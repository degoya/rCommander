//! Tar, plain or inside a compressed stream: GNU long names and PAX paths included.
//!
//! A tar has no index: the listing walks it once, and reading an entry walks it again up to that
//! entry - a reading session ([`serve`]) walks on from the entry before while the reads go
//! forward, so a whole tar read in order is walked once. A plain tar is walked with seeks over the
//! contents; a compressed one must be unpacked all the way, behind the guard ([`super::decode`])
//! that stops a bomb.

use std::{
    io::{BufReader, Read},
    path::Path,
};

use tar::{Archive, Entries};

use super::{
    ArchiveLimits, ArchiveSettings, Compression, decode,
    pipe::pump,
    safety::Budget,
    session::{Feed, Request, not_found},
    source::{Source, io_error},
    tree::{RawEntry, RawKind, permission_bits, readonly_mode},
};
use crate::{Result, unix_time};

/// One entry's header, read before its contents.
type Visit<'a> = dyn FnMut(u64, RawEntry, &mut dyn Read) -> Result<bool> + 'a;

/// Every entry of the tar.
pub(crate) fn scan(
    source: &Source,
    compression: Option<Compression>,
    settings: &ArchiveSettings,
) -> Result<Vec<RawEntry>> {
    let path = source.path();
    let mut budget = Budget::new(&settings.limits, path);
    let mut entries = Vec::new();
    walk(source, compression, settings.limits, &mut |_, entry, _| {
        budget.admit(entry.size)?;
        entries.push(entry);
        Ok(true)
    })?;
    Ok(entries)
}

/// Serves `first` and every request after it: one walk while the requests go forward, a new
/// walk from the start for one behind.
pub(crate) fn serve(
    source: &Source,
    compression: Option<Compression>,
    limits: ArchiveLimits,
    first: Request,
    feed: &mut Feed,
) {
    let mut pending = Some(first);
    while let Some(request) = pending.take() {
        let mut current = Some(request);
        let walked = walk(source, compression, limits, &mut |position, _, data| {
            let Some(index) = current.as_ref().map(|request| request.wanted.index) else {
                return Ok(false);
            };
            if position < index {
                return Ok(true);
            }
            if position > index {
                // Walked past it: not found, below.
                return Ok(false);
            }
            let Some(request) = current.take() else {
                return Ok(false);
            };
            let outcome = pump(
                data,
                request.wanted.size,
                &request.writer,
                &request.wanted.path,
            );
            match feed.finish(request, outcome) {
                Some(next) if next.wanted.index > position => {
                    current = Some(next);
                    Ok(true)
                }
                next => {
                    pending = next;
                    Ok(false)
                }
            }
        });
        if let Some(request) = current {
            let error = walked
                .err()
                .unwrap_or_else(|| not_found(&request.wanted.path));
            return request.fail(error);
        }
    }
}

/// Calls `visit` with each entry's position, header and contents until it returns `false`.
fn walk(
    source: &Source,
    compression: Option<Compression>,
    limits: ArchiveLimits,
    visit: &mut Visit<'_>,
) -> Result<()> {
    let path = source.path();
    match compression {
        None => {
            let mut archive = Archive::new(source.seekable()?);
            let entries = archive
                .entries_with_seek()
                .map_err(|error| io_error(error, path))?;
            visit_all(entries, visit, path)
        }
        Some(compression) => {
            let stream = BufReader::new(source.stream()?);
            let guard = decode::guarded(compression, stream, limits, path)
                .map_err(|error| io_error(error, path))?;
            let mut archive = Archive::new(guard);
            let entries = archive.entries().map_err(|error| io_error(error, path))?;
            visit_all(entries, visit, path)
        }
    }
}

fn visit_all<R: Read>(entries: Entries<'_, R>, visit: &mut Visit<'_>, path: &Path) -> Result<()> {
    for (position, entry) in (0_u64..).zip(entries) {
        let mut entry = entry.map_err(|error| io_error(error, path))?;
        let header = describe(&entry, position);
        if !visit(position, header, &mut entry)? {
            break;
        }
    }
    Ok(())
}

fn describe<R: Read>(entry: &tar::Entry<'_, R>, position: u64) -> RawEntry {
    let header = entry.header();
    let name = String::from_utf8_lossy(&entry.path_bytes()).into_owned();
    let kind = header.entry_type();
    let kind = if kind.is_dir() || (kind.is_file() && name.ends_with('/')) {
        // An old tar marks a directory by its trailing slash alone.
        RawKind::Dir
    } else if kind.is_symlink() {
        let target = entry
            .link_name_bytes()
            .map(|target| String::from_utf8_lossy(&target).into_owned())
            .unwrap_or_default();
        RawKind::Symlink(target)
    } else if kind.is_file() || kind.is_contiguous() || kind.is_gnu_sparse() {
        RawKind::File
    } else {
        // Hard links, devices, FIFOs: shown, not read.
        RawKind::Other
    };
    let mode = header.mode().ok().map(permission_bits);
    let mut raw = RawEntry::new(name, kind, position);
    raw.size = entry.size();
    raw.modified = header.mtime().ok().and_then(unix_time);
    raw.mode = mode;
    raw.readonly = readonly_mode(mode);
    raw
}

#[cfg(test)]
#[path = "tar_format_tests.rs"]
mod tests;
