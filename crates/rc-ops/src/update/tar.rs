//! Plain tar: each kept entry copied byte for byte with the extension headers before it (GNU long
//! names and link names, PAX records such as exact times), the new entries appended as a pack
//! writes them, then the closing blocks. A tar with sparse files is refused: its extension blocks
//! are not walked here.

use std::{
    fs::File,
    io::{self, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
};

use super::{Old, Paced, Refused, Rewrite, Stats, normalize, open_old, refused};
use crate::pack::{self, write::Feed};

/// A tar's block.
const BLOCK: u64 = 512;

/// One entry, where its bytes lie: from the first extension header before it to the end of its
/// padded data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Span {
    pub(super) start: u64,
    pub(super) end: u64,
}

/// Every entry of the tar, at the positions the archive backend lists them by.
pub(super) fn scan(path: &Path) -> io::Result<Vec<Old>> {
    Ok(walk(path)?.into_iter().map(|(old, _)| old).collect())
}

/// Every entry with where it lies.
pub(super) fn walk(path: &Path) -> io::Result<Vec<(Old, Span)>> {
    let mut archive = ::tar::Archive::new(open_old(path)?);
    let mut entries = Vec::new();
    let mut start = 0_u64;
    for (position, entry) in (0_u64..).zip(archive.entries_with_seek()?) {
        let entry = entry?;
        let header = entry.header();
        let kind = header.entry_type();
        if kind.is_gnu_sparse() {
            return Err(refused(Refused::NotModifiable));
        }
        let name = String::from_utf8_lossy(&entry.path_bytes()).into_owned();
        let size = entry.size();
        let end = size
            .checked_next_multiple_of(BLOCK)
            .and_then(|padded| entry.raw_file_position().checked_add(padded))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "a tar entry too large"))?;
        entries.push((
            Old {
                index: position,
                dir: kind.is_dir() || name.ends_with('/'),
                name: normalize(&name),
                size,
                modified: header.mtime().ok().and_then(rc_vfs::unix_time),
            },
            Span { start, end },
        ));
        start = end;
    }
    Ok(entries)
}

/// Writes the new tar into `out`: the entries not removed as they lie, then the new ones.
pub(super) fn rewrite(
    job: &Rewrite,
    feed: &Feed,
    mut out: BufWriter<File>,
) -> io::Result<(File, Stats)> {
    let spans = walk(&job.old)?;
    let mut source = Paced::new(open_old(&job.old)?, feed);
    let mut stats = Stats::default();
    for (old, span) in &spans {
        if job.remove.contains(&old.index) {
            continue;
        }
        source.seek(SeekFrom::Start(span.start))?;
        let length = span.end - span.start;
        let copied = io::copy(&mut (&mut source).take(length), &mut out)?;
        if copied != length {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the archive got shorter while it was changed",
            ));
        }
        stats.raw += 1;
    }
    let mut out = pack::tar::write_plain(out, &job.members, feed)?;
    out.flush()?;
    stats.entries = stats.raw + job.members.len() as u64;
    let file = out.into_inner().map_err(io::IntoInnerError::into_error)?;
    Ok((file, stats))
}

/// Reads the new tar back: every header (the reader checks each one's checksum), as many entries
/// as written, the new ones read through to their end.
pub(super) fn verify(job: &Rewrite, stats: &Stats) -> io::Result<()> {
    let mut archive = ::tar::Archive::new(open_old(&job.temp)?);
    let mut count = 0_u64;
    for entry in archive.entries_with_seek()? {
        let mut entry = entry.map_err(|_| refused(Refused::Unverified))?;
        if count >= stats.raw {
            let size = entry.size();
            let read =
                io::copy(&mut entry, &mut io::sink()).map_err(|_| refused(Refused::Unverified))?;
            if read != size {
                return Err(refused(Refused::Unverified));
            }
        }
        count += 1;
    }
    if count != stats.entries {
        return Err(refused(Refused::Unverified));
    }
    Ok(())
}

#[cfg(test)]
#[path = "tar_tests.rs"]
mod tests;
