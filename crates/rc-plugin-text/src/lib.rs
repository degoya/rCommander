//! The text encodings rCommander's bundled plugins read (RC-100-22), so each viewer does not
//! carry code-page tables of its own.
//!
//! A viewer writes UTF-8 into its sink whatever the file is written in, and its token offsets are
//! UTF-8 bytes of exactly that text. [`detect`] tells the encoding from a file's first bytes,
//! [`Decoder`] turns the file into that text window by window. What is not valid in the encoding
//! becomes U+FFFD, as `String::from_utf8_lossy` and `String::from_utf16_lossy` write it.
//!
//! The crate has no dependency and builds for `wasm32-unknown-unknown`: a plugin links it into its
//! component with `rc-plugin-text.workspace = true`.

mod decoder;
mod tables;

use std::fmt;
use std::str::FromStr;

pub use decoder::Decoder;

/// An encoding a file is read in. `auto`, a setting's usual default, is none of them: the caller
/// resolves it with [`detect`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Encoding {
    #[default]
    Utf8,
    Utf16Le,
    Utf16Be,
    /// The ANSI code page of Western European Windows.
    Windows1252,
    /// The OEM code page of German and other Western European Windows.
    Cp850,
    /// The OEM code page of US Windows.
    Cp437,
}

/// A setting value that names no [`Encoding`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownEncoding;

impl fmt::Display for UnknownEncoding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("unknown encoding")
    }
}

impl std::error::Error for UnknownEncoding {}

impl FromStr for Encoding {
    type Err = UnknownEncoding;

    /// The values a plugin's `encoding` setting offers: `utf-8`, `utf-16le`, `utf-16be`,
    /// `windows-1252`, `cp850`, `cp437`.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "utf-8" => Ok(Self::Utf8),
            "utf-16le" => Ok(Self::Utf16Le),
            "utf-16be" => Ok(Self::Utf16Be),
            "windows-1252" => Ok(Self::Windows1252),
            "cp850" => Ok(Self::Cp850),
            "cp437" => Ok(Self::Cp437),
            _ => Err(UnknownEncoding),
        }
    }
}

const BOM_UTF8: &[u8] = b"\xEF\xBB\xBF";
const BOM_UTF16LE: &[u8] = b"\xFF\xFE";
const BOM_UTF16BE: &[u8] = b"\xFE\xFF";

/// The encoding of a file whose first bytes are `prefix`, and the length of its byte-order mark,
/// which the text leaves out: a byte-order mark of UTF-8 or UTF-16 names its encoding; without
/// one the file is UTF-8 when `prefix` is valid UTF-8 (a character the prefix cuts at its end
/// counts as valid), else `fallback`.
#[must_use]
pub fn detect(prefix: &[u8], fallback: Encoding) -> (Encoding, usize) {
    if prefix.starts_with(BOM_UTF8) {
        return (Encoding::Utf8, BOM_UTF8.len());
    }
    if prefix.starts_with(BOM_UTF16LE) {
        return (Encoding::Utf16Le, BOM_UTF16LE.len());
    }
    if prefix.starts_with(BOM_UTF16BE) {
        return (Encoding::Utf16Be, BOM_UTF16BE.len());
    }
    let utf8 = match std::str::from_utf8(prefix) {
        Ok(_) => true,
        Err(error) => error.error_len().is_none(),
    };
    (if utf8 { Encoding::Utf8 } else { fallback }, 0)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
