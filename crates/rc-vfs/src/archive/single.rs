//! A single compressed file (`.gz`, `.xz`, `.bz2`, `.zst`) as an archive of one entry: the
//! file's name without the compression's suffix.
//!
//! The formats store no reliable size, so the listing unpacks the stream once to count it,
//! behind the guard that stops a bomb; the time is the compressed file's own.

use std::{
    io::{self, BufReader},
    time::SystemTime,
};

use super::{
    ArchiveLimits, ArchiveSettings, Compression, decode,
    pipe::{Pumped, pump},
    safety::Budget,
    session::{Feed, Request},
    source::{Source, io_error},
    tree::{RawEntry, RawKind},
};
use crate::Result;

/// What the listing takes from the compressed file itself.
#[derive(Debug, Clone)]
pub(crate) struct Outer {
    pub name: String,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// The one entry.
pub(crate) fn scan(
    source: &Source,
    compression: Compression,
    settings: &ArchiveSettings,
    outer: &Outer,
) -> Result<Vec<RawEntry>> {
    let path = source.path();
    let mut stream = open(source, compression, settings.limits)?;
    let size = io::copy(&mut stream, &mut io::sink()).map_err(|error| io_error(error, path))?;
    Budget::new(&settings.limits, path).admit(size)?;
    let mut entry = RawEntry::new(inner_name(&outer.name, compression), RawKind::File, 0);
    entry.size = size;
    entry.packed = Some(outer.size);
    entry.modified = outer.modified;
    Ok(vec![entry])
}

/// Serves `first` and every request after it, each from the start: there is one entry.
pub(crate) fn serve(
    source: &Source,
    compression: Compression,
    limits: ArchiveLimits,
    first: Request,
    feed: &mut Feed,
) {
    let mut request = first;
    loop {
        let outcome = read(source, compression, limits, &request);
        match feed.finish(request, outcome) {
            Some(next) => request = next,
            None => return,
        }
    }
}

/// Streams the entry into the pipe.
fn read(
    source: &Source,
    compression: Compression,
    limits: ArchiveLimits,
    request: &Request,
) -> Result<Pumped> {
    let mut stream = open(source, compression, limits)?;
    pump(
        &mut stream,
        request.wanted.size,
        &request.writer,
        &request.wanted.path,
    )
}

fn open(source: &Source, compression: Compression, limits: ArchiveLimits) -> Result<decode::Guard> {
    let path = source.path();
    let stream = BufReader::new(source.stream()?);
    decode::guarded(compression, stream, limits, path).map_err(|error| io_error(error, path))
}

/// The entry's name: `notes.txt.gz` holds `notes.txt`, `backup.tgz` holds `backup.tar`. A name
/// without a known suffix loses its last extension, if it has one.
pub(crate) fn inner_name(outer: &str, compression: Compression) -> String {
    let suffixes: &[(&str, &str)] = match compression {
        Compression::Gzip => &[(".tgz", ".tar"), (".gz", ""), (".gzip", "")],
        Compression::Xz => &[(".txz", ".tar"), (".xz", "")],
        Compression::Bzip2 => &[
            (".tbz2", ".tar"),
            (".tbz", ".tar"),
            (".bz2", ""),
            (".bzip2", ""),
        ],
        Compression::Zstd => &[(".tzst", ".tar"), (".zst", ""), (".zstd", "")],
    };
    let lower = outer.to_ascii_lowercase();
    for (suffix, replacement) in suffixes {
        if lower.ends_with(suffix) && outer.len() > suffix.len() {
            // The suffix is ASCII and `lower` keeps every byte position, so this is a boundary.
            return format!("{}{replacement}", &outer[..outer.len() - suffix.len()]);
        }
    }
    match outer.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_owned(),
        _ => outer.to_owned(),
    }
}

#[cfg(test)]
#[path = "single_tests.rs"]
mod tests;
