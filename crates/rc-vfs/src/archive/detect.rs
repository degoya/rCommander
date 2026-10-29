//! Which archive a file is, from its first bytes, never from its name: a `.zip` that is not a
//! zip is not opened as one, and a file of no known format is no archive.
//!
//! A compressed stream is unpacked far enough to see whether a tar header starts it: then it is
//! a compressed tar, otherwise a single compressed file.

use std::io::{self, Cursor, Read};

use super::{ArchiveFormat, ArchiveLimits, Compression, decode};

/// A tar header block.
const BLOCK: usize = 512;

const ZIP: &[&[u8]] = &[b"PK\x03\x04", b"PK\x05\x06", b"PK\x07\x08"];
const SEVEN_Z: &[u8] = b"7z\xBC\xAF\x27\x1C";
const GZIP: &[u8] = b"\x1F\x8B";
const XZ: &[u8] = b"\xFD7zXZ\x00";
const ZSTD: &[u8] = b"\x28\xB5\x2F\xFD";

/// The format of the stream `reader` starts; `None` for anything that is no archive. An xz
/// stream whose dictionary passes the decoder memory of `limits` cannot be looked into: its
/// `OutOfMemory` is the answer.
pub(crate) fn sniff<R: Read + 'static>(
    mut reader: R,
    limits: &ArchiveLimits,
) -> io::Result<Option<ArchiveFormat>> {
    let mut head = Vec::with_capacity(BLOCK);
    reader.by_ref().take(BLOCK as u64).read_to_end(&mut head)?;
    if ZIP.iter().any(|magic| head.starts_with(magic)) {
        return Ok(Some(ArchiveFormat::Zip));
    }
    if head.starts_with(SEVEN_Z) {
        return Ok(Some(ArchiveFormat::SevenZ));
    }
    if let Some(compression) = compression(&head) {
        let whole = Cursor::new(head).chain(reader);
        let tar = match decode::decoder(compression, whole, limits) {
            Ok(decoder) => {
                let mut inner = Vec::with_capacity(BLOCK);
                // A stream that breaks before a whole header is no tar; it may still be a
                // (damaged) single file, which its listing will report.
                if let Err(error) = decoder.take(BLOCK as u64).read_to_end(&mut inner)
                    && error.kind() == io::ErrorKind::OutOfMemory
                {
                    return Err(error);
                }
                is_tar(&inner)
            }
            Err(_) => false,
        };
        return Ok(Some(if tar {
            ArchiveFormat::Tar {
                compression: Some(compression),
            }
        } else {
            ArchiveFormat::Single { compression }
        }));
    }
    Ok(is_tar(&head).then_some(ArchiveFormat::Tar { compression: None }))
}

/// The compression whose magic starts `head`.
fn compression(head: &[u8]) -> Option<Compression> {
    if head.starts_with(GZIP) {
        Some(Compression::Gzip)
    } else if head.starts_with(XZ) {
        Some(Compression::Xz)
    } else if head.starts_with(ZSTD) {
        Some(Compression::Zstd)
    } else if head.len() >= 4 && head.starts_with(b"BZh") && (b'1'..=b'9').contains(&head[3]) {
        Some(Compression::Bzip2)
    } else {
        None
    }
}

/// Whether `block` starts with a tar header: the POSIX `ustar` magic, or for an old v7 tar a
/// checksum that adds up.
pub(crate) fn is_tar(block: &[u8]) -> bool {
    let Some(block) = block.get(..BLOCK) else {
        return false;
    };
    if &block[257..262] == b"ustar" {
        return true;
    }
    let Some(stored) = octal(&block[148..156]) else {
        return false;
    };
    // The checksum counts its own field as eight spaces.
    let sum: u64 = block
        .iter()
        .enumerate()
        .map(|(index, &byte)| {
            if (148..156).contains(&index) {
                u64::from(b' ')
            } else {
                u64::from(byte)
            }
        })
        .sum();
    block[0] != 0 && sum == stored
}

/// An octal header field, padded with NULs or spaces.
fn octal(field: &[u8]) -> Option<u64> {
    let text = std::str::from_utf8(field).ok()?;
    let digits = text.trim_matches(|c: char| c == '\0' || c == ' ');
    if digits.is_empty() {
        return None;
    }
    u64::from_str_radix(digits, 8).ok()
}

#[cfg(test)]
#[path = "detect_tests.rs"]
mod tests;
