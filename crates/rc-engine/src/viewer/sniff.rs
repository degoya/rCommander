//! What a file is, from its first bytes: which image format, and which view mode suits it.
//!
//! Only the signature is read here; decoding an image (and saying it is broken) belongs to the
//! window that shows it.

use serde::{Deserialize, Serialize};

use super::{Detected, encoding::TextEncoding};

/// The image formats the viewer shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    WebP,
    Bmp,
    Ico,
}

impl ImageFormat {
    /// The name for the status bar.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Gif => "GIF",
            Self::WebP => "WebP",
            Self::Bmp => "BMP",
            Self::Ico => "ICO",
        }
    }
}

/// The view modes, numbered as Lister's keys 1 to 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    Text,
    /// Fixed-width rows of characters, no line breaks.
    Binary,
    Hex,
    Image,
}

/// The image format whose signature `head` starts with.
#[must_use]
pub fn sniff_image(head: &[u8]) -> Option<ImageFormat> {
    if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(ImageFormat::Png)
    } else if head.starts_with(b"\xFF\xD8\xFF") {
        Some(ImageFormat::Jpeg)
    } else if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        Some(ImageFormat::Gif)
    } else if head.len() >= 12 && head.starts_with(b"RIFF") && &head[8..12] == b"WEBP" {
        Some(ImageFormat::WebP)
    } else if is_bmp(head) {
        Some(ImageFormat::Bmp)
    } else if is_ico(head) {
        Some(ImageFormat::Ico)
    } else {
        None
    }
}

/// `BM` and a known size of the info header behind the 14-byte file header, so a text file
/// starting with "BM" is not taken for a bitmap.
fn is_bmp(head: &[u8]) -> bool {
    head.len() >= 18
        && head.starts_with(b"BM")
        && matches!(
            u32::from_le_bytes([head[14], head[15], head[16], head[17]]),
            12 | 40 | 52 | 56 | 64 | 108 | 124
        )
}

/// Reserved 0, type 1 (icon), at least one image, and the first entry's reserved byte 0.
fn is_ico(head: &[u8]) -> bool {
    head.len() >= 22
        && head.starts_with(b"\x00\x00\x01\x00")
        && u16::from_le_bytes([head[4], head[5]]) > 0
        && head[9] == 0
}

/// Whether `head` looks like binary data rather than text: a zero byte after the byte-order
/// mark, which text in a single-byte encoding or UTF-8 never has.
#[must_use]
pub fn looks_binary(head: &[u8], detected: Detected) -> bool {
    if matches!(
        detected.encoding,
        TextEncoding::Utf16Le | TextEncoding::Utf16Be
    ) {
        return false;
    }
    let skip = usize::try_from(detected.bom_len)
        .unwrap_or(usize::MAX)
        .min(head.len());
    head[skip..].contains(&0)
}

/// The mode a file opens in: an image as one, everything else as text - as Lister does.
#[must_use]
pub fn suggest_mode(head: &[u8]) -> ViewMode {
    if sniff_image(head).is_some() {
        ViewMode::Image
    } else {
        ViewMode::Text
    }
}

#[cfg(test)]
#[path = "sniff_tests.rs"]
mod tests;
