//! The central directory of a zip: per entry its name, method, flags, CRC, sizes and where its
//! local header is, with the Zip64 extra field where a 32-bit field is saturated. Read from the
//! bytes [`crate::entries`] located, at most [`MAX_DIRECTORY_BYTES`] of them.

use crate::entries::{le16, le32, le64};
use crate::source_io::damaged;
use crate::types::Failure;

/// The most bytes of central directory read: 10 000 entries of Office part names take a small
/// fraction of it.
pub(crate) const MAX_DIRECTORY_BYTES: u64 = 16 << 20;

const HEADER_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
const HEADER_BYTES: usize = 46;
const ZIP64_EXTRA: u16 = 0x0001;
const SATURATED: u32 = 0xFFFF_FFFF;

/// Packed as is.
pub(crate) const STORED: u16 = 0;
/// Raw Deflate.
pub(crate) const DEFLATED: u16 = 8;

/// One entry of the directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) name: String,
    pub(crate) flags: u16,
    pub(crate) method: u16,
    pub(crate) crc: u32,
    pub(crate) packed: u64,
    pub(crate) size: u64,
    /// The offset of its local header.
    pub(crate) header: u64,
}

impl Entry {
    /// Whether the entry is encrypted (general purpose flag bit 0).
    pub(crate) fn encrypted(&self) -> bool {
        self.flags & 1 != 0
    }

    /// Whether the entry is a folder.
    pub(crate) fn is_dir(&self) -> bool {
        self.name.ends_with('/')
    }
}

/// The `count` entries of the directory `bytes`; damaged when a header is broken or missing.
pub(crate) fn parse(bytes: &[u8], count: u64) -> Result<Vec<Entry>, Failure> {
    let mut entries = Vec::new();
    let mut at = 0usize;
    for _ in 0..count {
        let header = bytes.get(at..at + HEADER_BYTES).ok_or_else(damaged)?;
        if header.get(..4) != Some(&HEADER_SIGNATURE[..]) {
            return Err(damaged());
        }
        let name_bytes = usize::from(le16(header, 28));
        let extra_bytes = usize::from(le16(header, 30));
        let comment_bytes = usize::from(le16(header, 32));
        let name_start = at + HEADER_BYTES;
        let extra_start = name_start + name_bytes;
        let next = extra_start + extra_bytes + comment_bytes;
        let name = bytes.get(name_start..extra_start).ok_or_else(damaged)?;
        let extra = bytes
            .get(extra_start..extra_start + extra_bytes)
            .ok_or_else(damaged)?;
        if next > bytes.len() {
            return Err(damaged());
        }
        let mut entry = Entry {
            name: String::from_utf8_lossy(name).into_owned(),
            flags: le16(header, 8),
            method: le16(header, 10),
            crc: le32(header, 16),
            packed: u64::from(le32(header, 20)),
            size: u64::from(le32(header, 24)),
            header: u64::from(le32(header, 42)),
        };
        apply_zip64(&mut entry, header, extra)?;
        entries.push(entry);
        at = next;
    }
    Ok(entries)
}

/// Takes the 64-bit sizes and offset from the Zip64 extra field for each 32-bit field that is
/// saturated, in the order the format lists them: size, packed size, header offset.
fn apply_zip64(entry: &mut Entry, header: &[u8], extra: &[u8]) -> Result<(), Failure> {
    let wants = [
        le32(header, 24) == SATURATED,
        le32(header, 20) == SATURATED,
        le32(header, 42) == SATURATED,
    ];
    if !wants.contains(&true) {
        return Ok(());
    }
    let mut at = 0usize;
    while at + 4 <= extra.len() {
        let id = le16(extra, at);
        let length = usize::from(le16(extra, at + 2));
        let data = extra.get(at + 4..at + 4 + length).ok_or_else(damaged)?;
        if id == ZIP64_EXTRA {
            let mut field = 0usize;
            for (want, slot) in
                wants
                    .into_iter()
                    .zip([&mut entry.size, &mut entry.packed, &mut entry.header])
            {
                if want {
                    if field + 8 > data.len() {
                        return Err(damaged());
                    }
                    *slot = le64(data, field);
                    field += 8;
                }
            }
            return Ok(());
        }
        at += 4 + length;
    }
    Err(damaged())
}

#[cfg(test)]
#[path = "central_tests.rs"]
mod tests;
