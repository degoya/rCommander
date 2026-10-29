//! Total Commander's `.crc` file beside split parts: the original name, its size and its
//! CRC32, one `key=value` per line.
//!
//! ```text
//! filename=disk.iso
//! size=5368709120
//! crc32=1A2B3C4D
//! ```

use std::path::Path;

use crate::{ChecksumError, names};

/// The largest `.crc` file read; Total Commander's are a few dozen bytes.
pub(crate) const MAX_CRC_FILE: u64 = 64 * 1024;

/// What a `.crc` file says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CrcInfo {
    /// The original file name, checked to be a plain name.
    pub(crate) file_name: String,
    /// The original size, when the file names it.
    pub(crate) size: Option<u64>,
    /// The CRC32 as eight lowercase hex digits.
    pub(crate) crc32: String,
}

/// The `.crc` file's text, with Total Commander's CRLF line ends.
pub(crate) fn write(file_name: &str, size: u64, crc32: &str) -> String {
    format!(
        "filename={file_name}\r\nsize={size}\r\ncrc32={}\r\n",
        crc32.to_ascii_uppercase()
    )
}

/// Parses `text`, the `.crc` file at `path`. Keys are matched ignoring case, unknown keys are
/// skipped; `filename` and `crc32` are required.
pub(crate) fn parse(path: &Path, text: &str) -> Result<CrcInfo, ChecksumError> {
    let invalid = |reason| ChecksumError::InvalidCrcFile {
        path: path.to_path_buf(),
        reason,
    };
    let (mut file_name, mut size, mut crc32) = (None, None, None);
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "filename" => {
                let name = names::plain(value).map_err(|_| invalid("file name"))?;
                file_name = Some(name.to_owned());
            }
            "size" => size = Some(value.parse::<u64>().map_err(|_| invalid("size"))?),
            "crc32" => {
                let valid = !value.is_empty() && value.len() <= 8;
                let crc = u32::from_str_radix(value, 16)
                    .ok()
                    .filter(|_| valid)
                    .ok_or_else(|| invalid("crc32"))?;
                crc32 = Some(format!("{crc:08x}"));
            }
            _ => {}
        }
    }
    Ok(CrcInfo {
        file_name: file_name.ok_or_else(|| invalid("no filename line"))?,
        size,
        crc32: crc32.ok_or_else(|| invalid("no crc32 line"))?,
    })
}

#[cfg(test)]
#[path = "crc_file_tests.rs"]
mod tests;
