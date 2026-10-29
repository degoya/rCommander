//! Zip: deflate at the level asked (0 stores), zip64 for a file of 4 GiB or more, UTF-8 names.
//!
//! The time is the DOS time of the zip header, local and in two-second steps, as Windows and
//! 7-Zip read it; the Unix permissions go into the external attributes.

use std::{
    fs::File,
    io::{self, BufWriter},
    time::SystemTime,
};

use chrono::{Datelike, Local, Timelike};
use zip::{CompressionMethod, DateTime, ZipWriter, write::SimpleFileOptions};

use super::write::{Feed, Member, MemberKind};

/// Writes `members` as a zip into `out`; the file back once the central directory is written.
pub(super) fn write(
    out: BufWriter<File>,
    members: &[Member],
    level: u32,
    feed: &Feed,
) -> io::Result<File> {
    let mut zip = ZipWriter::new(out);
    for member in members {
        let options = options(member, level);
        match &member.kind {
            MemberKind::Dir => zip.add_directory(member.name.as_str(), options)?,
            MemberKind::Link(target) => {
                zip.add_symlink(member.name.as_str(), target.as_str(), options)?;
            }
            MemberKind::File => {
                zip.start_file(member.name.as_str(), options)?;
                let mut source = feed.open(member)?;
                io::copy(&mut source, &mut zip)?;
            }
        }
    }
    zip.finish()?
        .into_inner()
        .map_err(io::IntoInnerError::into_error)
}

fn options(member: &Member, level: u32) -> SimpleFileOptions {
    let mut options = SimpleFileOptions::default()
        .last_modified_time(dos_time(member.modified))
        .large_file(member.size >= u64::from(u32::MAX));
    options = if level == 0 {
        options.compression_method(CompressionMethod::Stored)
    } else {
        options
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(i64::from(level)))
    };
    match member.mode {
        Some(mode) => options.unix_permissions(mode),
        // Read-only without a Unix mode (Windows): no write bit for anyone.
        None if member.readonly && member.kind == MemberKind::File => {
            options.unix_permissions(0o444)
        }
        None => options,
    }
}

/// `time` as a DOS time in this machine's zone; 1980-01-01 where it does not fit (before 1980,
/// after 2107, or unknown).
pub(super) fn dos_time(time: Option<SystemTime>) -> DateTime {
    time.and_then(|time| {
        let local = chrono::DateTime::<Local>::from(time);
        DateTime::from_date_and_time(
            u16::try_from(local.year()).ok()?,
            u8::try_from(local.month()).ok()?,
            u8::try_from(local.day()).ok()?,
            u8::try_from(local.hour()).ok()?,
            u8::try_from(local.minute()).ok()?,
            u8::try_from(local.second()).ok()?,
        )
        .ok()
    })
    .unwrap_or_default()
}

#[cfg(test)]
#[path = "zip_tests.rs"]
mod tests;
