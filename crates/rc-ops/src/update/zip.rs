//! Zip: the kept entries copied raw - their compressed bytes, CRC, sizes, extra fields and
//! comments as stored, nothing inflated - the archive's comment kept, the new entries deflated
//! as a pack deflates them.

use std::{
    fs::File,
    io::{self, BufWriter},
    path::Path,
    time::SystemTime,
};

use chrono::{Local, NaiveDate, TimeZone};
use zip::{DateTime, ZipArchive, ZipWriter};

use super::{Old, Paced, Refused, Rewrite, Stats, normalize, open_old, refused};
use crate::pack::{self, write::Feed};

/// Every entry of the zip; one behind a prefix (a self-extractor) or an encrypted one refuses
/// the archive.
pub(super) fn scan(path: &Path) -> io::Result<Vec<Old>> {
    let mut archive = ZipArchive::new(open_old(path)?)?;
    if archive.offset() > 0 {
        return Err(refused(Refused::NotModifiable));
    }
    let mut entries = Vec::with_capacity(archive.len().min(4096));
    for index in 0..archive.len() {
        let member = archive.by_index_raw(index)?;
        if member.encrypted() {
            return Err(refused(Refused::Encrypted));
        }
        let name = member.name().map_or_else(
            |_| String::from_utf8_lossy(member.name_raw()).into_owned(),
            std::borrow::Cow::into_owned,
        );
        entries.push(Old {
            index: index as u64,
            name: normalize(&name),
            dir: member.is_dir(),
            size: member.size(),
            modified: member.last_modified().and_then(dos_time),
        });
    }
    Ok(entries)
}

/// Writes the new zip into `out`: the entries not removed raw, then the new ones.
pub(super) fn rewrite(
    job: &Rewrite,
    feed: &Feed,
    out: BufWriter<File>,
) -> io::Result<(File, Stats)> {
    let mut archive = ZipArchive::new(Paced::new(open_old(&job.old)?, feed))?;
    if archive.offset() > 0 {
        return Err(refused(Refused::NotModifiable));
    }
    let mut zip = ZipWriter::new(out);
    zip.set_raw_comment(Box::from(archive.comment()))?;
    let mut stats = Stats::default();
    for index in 0..archive.len() {
        if job.remove.contains(&(index as u64)) {
            continue;
        }
        let member = archive.by_index_raw(index)?;
        if member.encrypted() {
            return Err(refused(Refused::Encrypted));
        }
        zip.raw_copy_file(member)?;
        stats.raw += 1;
    }
    for member in &job.members {
        pack::zip::add_member(&mut zip, member, job.level, feed)?;
    }
    stats.entries = stats.raw + job.members.len() as u64;
    let file = zip
        .finish()?
        .into_inner()
        .map_err(io::IntoInnerError::into_error)?;
    Ok((file, stats))
}

/// Reads the new zip back: as many entries as written, each kept one with the CRC and sizes it
/// had, each new one read through to its end, where the reader checks its CRC.
pub(super) fn verify(job: &Rewrite, stats: &Stats) -> io::Result<()> {
    let mut old = ZipArchive::new(open_old(&job.old)?)?;
    let mut new = ZipArchive::new(open_old(&job.temp)?)?;
    if new.len() as u64 != stats.entries {
        return Err(refused(Refused::Unverified));
    }
    let mut at = 0;
    for index in 0..old.len() {
        if job.remove.contains(&(index as u64)) {
            continue;
        }
        let was = old.by_index_raw(index)?;
        let is = new.by_index_raw(at)?;
        let same = was.crc32() == is.crc32()
            && was.compressed_size() == is.compressed_size()
            && was.size() == is.size();
        if !same {
            return Err(refused(Refused::Unverified));
        }
        at += 1;
    }
    for index in at..new.len() {
        let mut member = new.by_index(index)?;
        let size = member.size();
        let read =
            io::copy(&mut member, &mut io::sink()).map_err(|_| refused(Refused::Unverified))?;
        if read != size {
            return Err(refused(Refused::Unverified));
        }
    }
    Ok(())
}

/// A DOS date and time, as this machine's local time: what the conflict question shows.
fn dos_time(stamp: DateTime) -> Option<SystemTime> {
    let naive = NaiveDate::from_ymd_opt(
        i32::from(stamp.year()),
        u32::from(stamp.month()),
        u32::from(stamp.day()),
    )?
    .and_hms_opt(
        u32::from(stamp.hour()),
        u32::from(stamp.minute()),
        u32::from(stamp.second()),
    )?;
    let local = Local.from_local_datetime(&naive).earliest()?;
    Some(SystemTime::from(local))
}

#[cfg(test)]
#[path = "zip_tests.rs"]
mod tests;
