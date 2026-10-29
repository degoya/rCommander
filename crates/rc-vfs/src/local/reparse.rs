//! The data of a junction: the mount-point reparse buffer `FSCTL_SET_REPARSE_POINT` takes, built
//! without a Win32 call so it is tested on every platform.
//!
//! Layout (`REPARSE_DATA_BUFFER`, little endian): tag (u32), data length (u16), reserved (u16),
//! then substitute-name offset and length, print-name offset and length (u16 each, in bytes
//! into the path buffer), then the path buffer: `\??\C:\target` NUL `C:\target` NUL in UTF-16.

use std::io;

/// `IO_REPARSE_TAG_MOUNT_POINT`.
pub(super) const MOUNT_POINT_TAG: u32 = 0xA000_0003;

/// `MAXIMUM_REPARSE_DATA_BUFFER_SIZE`: 16 KiB, header included.
const MAX_BUFFER: usize = 16 * 1024;

/// The tag, length and reserved field before the mount-point fields.
const HEADER: usize = 8;

/// The four offsets and lengths before the path buffer.
const MOUNT_HEADER: usize = 8;

/// The buffer for a junction to `target`, an absolute local path (`C:\dir`, or `\\?\C:\dir`).
/// A network path cannot be a junction's target.
pub(super) fn mount_point_buffer(target: &str) -> io::Result<Vec<u8>> {
    let print = target.strip_prefix(r"\\?\").unwrap_or(target);
    let drive = print.as_bytes();
    if print.starts_with(r"\\") || drive.len() < 3 || drive[1] != b':' || drive[2] != b'\\' {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a junction leads to an absolute path on a local drive",
        ));
    }
    let substitute: Vec<u16> = format!(r"\??\{print}").encode_utf16().collect();
    let print: Vec<u16> = print.encode_utf16().collect();
    let substitute_bytes = substitute.len() * 2;
    let print_bytes = print.len() * 2;
    // Both names end in a NUL that the lengths do not count.
    let data_len = MOUNT_HEADER + substitute_bytes + 2 + print_bytes + 2;
    if HEADER + data_len > MAX_BUFFER {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the target's path is too long for a junction",
        ));
    }
    let short = |value: usize| {
        u16::try_from(value).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
    };
    let mut buffer = Vec::with_capacity(HEADER + data_len);
    buffer.extend_from_slice(&MOUNT_POINT_TAG.to_le_bytes());
    buffer.extend_from_slice(&short(data_len)?.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    buffer.extend_from_slice(&short(substitute_bytes)?.to_le_bytes());
    buffer.extend_from_slice(&short(substitute_bytes + 2)?.to_le_bytes());
    buffer.extend_from_slice(&short(print_bytes)?.to_le_bytes());
    for unit in substitute.into_iter().chain([0]).chain(print).chain([0]) {
        buffer.extend_from_slice(&unit.to_le_bytes());
    }
    Ok(buffer)
}

#[cfg(test)]
#[path = "reparse_tests.rs"]
mod tests;
