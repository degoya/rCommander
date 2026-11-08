//! The end record of a zip, read before anything else: how many entries it declares and where
//! its central directory lies. One read of the last 64 KiB finds every end-record signature in
//! it; the most entries any of them declares (with its Zip64 record) is what the entry limit is
//! checked on, so a decoy record in the comment cannot hide the count of another. The directory
//! is the one of the first record whose comment ends exactly at the end of the file, else of the
//! last record.

use crate::types::{Failure, Source};

const END_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const END_BYTES: usize = 22;
/// The end record and the longest comment.
const TAIL_BYTES: u64 = END_BYTES as u64 + 4 + (1 << 16);
const LOCATOR_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x06, 0x07];
const LOCATOR_BYTES: u64 = 20;
const END64_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x06, 0x06];
const END64_BYTES: u64 = 56;

/// What the end records say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct End {
    /// The most entries any end record in the tail declares.
    pub(crate) declared: u64,
    /// The entries of the chosen record's directory.
    pub(crate) entries: u64,
    /// Where the chosen record's central directory starts, and its bytes.
    pub(crate) offset: u64,
    pub(crate) size: u64,
}

/// The chosen record's fields.
#[derive(Clone, Copy)]
struct Record {
    entries: u64,
    offset: u64,
    size: u64,
}

/// The end records of the file; `None` when there is none, so the file is no zip.
pub(crate) fn find<S: Source>(src: &mut S) -> Result<Option<End>, Failure> {
    let file_size = src.size();
    if file_size < END_BYTES as u64 {
        return Ok(None);
    }
    let tail_start = file_size - file_size.min(TAIL_BYTES);
    let tail = read(src, tail_start, file_size - tail_start)?;
    let mut declared = 0u64;
    let mut exact: Option<Record> = None;
    let mut last: Option<Record> = None;
    for at in 0..=tail.len().saturating_sub(END_BYTES) {
        if tail.get(at..at + 4) != Some(&END_SIGNATURE[..]) {
            continue;
        }
        let Some(bytes) = tail.get(at..at + END_BYTES) else {
            continue;
        };
        let mut record = Record {
            entries: u64::from(le16(bytes, 8).max(le16(bytes, 10))),
            size: u64::from(le32(bytes, 12)),
            offset: u64::from(le32(bytes, 16)),
        };
        let saturated = le16(bytes, 8) == 0xFFFF
            || le16(bytes, 10) == 0xFFFF
            || le32(bytes, 12) == 0xFFFF_FFFF
            || le32(bytes, 16) == 0xFFFF_FFFF;
        if saturated && let Some(zip64) = zip64(src, tail_start + at as u64, file_size)? {
            record = zip64;
        }
        declared = declared.max(record.entries);
        let comment = usize::from(le16(bytes, 20));
        if exact.is_none() && at + END_BYTES + comment == tail.len() {
            exact = Some(record);
        }
        last = Some(record);
    }
    Ok(exact.or(last).map(|record| End {
        declared,
        entries: record.entries,
        offset: record.offset,
        size: record.size,
    }))
}

/// The Zip64 end record the locator before the end record at `end` points to.
fn zip64<S: Source>(src: &mut S, end: u64, file_size: u64) -> Result<Option<Record>, Failure> {
    let Some(locator_at) = end.checked_sub(LOCATOR_BYTES) else {
        return Ok(None);
    };
    let locator = read(src, locator_at, LOCATOR_BYTES)?;
    if locator.get(..4) != Some(&LOCATOR_SIGNATURE[..]) {
        return Ok(None);
    }
    let record_at = le64(&locator, 8);
    if record_at
        .checked_add(END64_BYTES)
        .is_none_or(|record_end| record_end > file_size)
    {
        return Ok(None);
    }
    let record = read(src, record_at, END64_BYTES)?;
    if record.get(..4) != Some(&END64_SIGNATURE[..]) {
        return Ok(None);
    }
    Ok(Some(Record {
        entries: le64(&record, 24).max(le64(&record, 32)),
        size: le64(&record, 40),
        offset: le64(&record, 48),
    }))
}

/// `length` bytes (at most [`TAIL_BYTES`]) at `offset`; fewer only at the end of the source.
fn read<S: Source>(src: &mut S, offset: u64, length: u64) -> Result<Vec<u8>, Failure> {
    let length = u32::try_from(length.min(TAIL_BYTES)).unwrap_or(0);
    let mut bytes = src.read_at(offset, length)?;
    bytes.truncate(length as usize);
    Ok(bytes)
}

/// The little-endian `u16` at `at`; 0 past the end.
pub(crate) fn le16(bytes: &[u8], at: usize) -> u16 {
    let mut le = [0u8; 2];
    if let Some(field) = bytes.get(at..at + 2) {
        le.copy_from_slice(field);
    }
    u16::from_le_bytes(le)
}

/// The little-endian `u32` at `at`; 0 past the end.
pub(crate) fn le32(bytes: &[u8], at: usize) -> u32 {
    let mut le = [0u8; 4];
    if let Some(field) = bytes.get(at..at + 4) {
        le.copy_from_slice(field);
    }
    u32::from_le_bytes(le)
}

/// The little-endian `u64` at `at`; 0 past the end.
pub(crate) fn le64(bytes: &[u8], at: usize) -> u64 {
    let mut le = [0u8; 8];
    if let Some(field) = bytes.get(at..at + 8) {
        le.copy_from_slice(field);
    }
    u64::from_le_bytes(le)
}

#[cfg(test)]
#[path = "entries_tests.rs"]
mod tests;
