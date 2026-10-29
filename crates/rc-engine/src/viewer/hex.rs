//! The hex view's rows: offset, bytes and their printable characters, as `xxd` prints them.

use std::fmt::Write as _;

use rc_vfs::VfsError;
use serde::Serialize;

use super::FileWindow;

/// Bytes per row, as in `xxd` and Total Commander's Lister.
pub const BYTES_PER_ROW: usize = 16;

/// One row of the hex view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HexRow {
    /// Where the row's first byte is.
    pub offset: u64,
    pub bytes: Vec<u8>,
}

impl HexRow {
    /// The bytes as `xxd` groups them: two bytes per group, groups apart by a space.
    #[must_use]
    pub fn hex(&self) -> String {
        let mut out = String::with_capacity(self.bytes.len() * 5 / 2);
        for (index, byte) in self.bytes.iter().enumerate() {
            if index > 0 && index.is_multiple_of(2) {
                out.push(' ');
            }
            let _ = write!(out, "{byte:02x}");
        }
        out
    }

    /// The bytes as characters: printable ASCII as itself, anything else as `.`.
    #[must_use]
    pub fn ascii(&self) -> String {
        self.bytes
            .iter()
            .map(|&byte| {
                if (0x20..0x7F).contains(&byte) {
                    char::from(byte)
                } else {
                    '.'
                }
            })
            .collect()
    }

    /// The row as `xxd` prints it for rows of `columns` bytes: offset, colon, the groups padded
    /// to a full row's width, two spaces, the characters.
    #[must_use]
    pub fn to_xxd(&self, columns: usize) -> String {
        let width = columns * 2 + columns.div_ceil(2).saturating_sub(1);
        format!(
            "{:08x}: {:<width$}  {}",
            self.offset,
            self.hex(),
            self.ascii()
        )
    }
}

/// `bytes`, which lie at `offset`, as rows of `columns` bytes starting there.
#[must_use]
pub fn rows(bytes: &[u8], offset: u64, columns: usize) -> Vec<HexRow> {
    bytes
        .chunks(columns.max(1))
        .zip((offset..).step_by(columns.max(1)))
        .map(|(chunk, at)| HexRow {
            offset: at,
            bytes: chunk.to_vec(),
        })
        .collect()
}

/// The start of the row holding `offset`, rows of `columns` bytes counted from 0.
#[must_use]
pub fn row_start(offset: u64, columns: usize) -> u64 {
    let columns = columns.max(1) as u64;
    offset - offset % columns
}

/// `count` rows of `columns` bytes, the first one holding `offset`; fewer at the end.
///
/// # Errors
///
/// The backend failed to read.
pub async fn read_rows(
    window: &FileWindow,
    offset: u64,
    count: usize,
    columns: usize,
) -> Result<Vec<HexRow>, VfsError> {
    let start = row_start(offset, columns);
    let bytes = window
        .read_at(start, count.saturating_mul(columns.max(1)))
        .await?;
    Ok(rows(&bytes, start, columns))
}

/// An offset as a person types it into "go to offset": hex with `0x`, `$` or a trailing `h`,
/// otherwise decimal. `None` for anything else.
#[must_use]
pub fn parse_offset(text: &str) -> Option<u64> {
    let text = text.trim();
    let lower = text.to_ascii_lowercase();
    let hex = lower
        .strip_prefix("0x")
        .or_else(|| lower.strip_prefix('$'))
        .or_else(|| lower.strip_suffix('h'));
    match hex {
        Some(digits) if !digits.is_empty() => u64::from_str_radix(digits, 16).ok(),
        Some(_) => None,
        None => text.parse().ok(),
    }
}

#[cfg(test)]
#[path = "hex_tests.rs"]
mod tests;
