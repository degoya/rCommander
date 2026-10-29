//! Which encoding a file's text is in, and decoding it: the one text decoding of the engine
//! (RC-080-21). The viewer, the compare window, `descript.ion` comments, the Total Commander
//! INI import and the content search's automatic mode all detect and decode here, so the same
//! bytes read the same everywhere.
//!
//! Detection looks at what it is given: a byte-order mark first, then UTF-16 without one
//! (ASCII-heavy text has a zero in every other byte), then UTF-8 validity; anything else is
//! Windows-1252, the ANSI code page Total Commander's Lister falls back to. Latin-1 is never
//! detected, only chosen; CP437 (DOS) is chosen, or taken for a file the setting names as DOS
//! text ([`detect_dos`], RC-020-11). The viewer detects from a file's head ([`detect`]), a caller
//! holding the whole file from all of it ([`detect_whole`], [`decode_text`]). Decoding is
//! `encoding_rs` but for Latin-1 and CP437, which it does not have (its `iso-8859-1` is
//! Windows-1252; CP437 is `cp437.rs`).

use encoding_rs::{UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252};
use serde::{Deserialize, Serialize};

use super::cp437;

/// How many bytes of a file's head [`detect`] wants to see.
pub const DETECT_BYTES: usize = 64 * 1024;

/// The encodings the viewer decodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextEncoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    /// ISO-8859-1: every byte is the code point of the same number.
    Latin1,
    Windows1252,
    /// Code page 437, DOS's: the box-drawing art of `.nfo` and `.diz` files.
    Cp437,
}

impl TextEncoding {
    /// Every encoding, in the order a choice lists them.
    pub const ALL: [Self; 6] = [
        Self::Utf8,
        Self::Utf16Le,
        Self::Utf16Be,
        Self::Latin1,
        Self::Windows1252,
        Self::Cp437,
    ];

    /// The usual name, for the status bar.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf16Le => "UTF-16 LE",
            Self::Utf16Be => "UTF-16 BE",
            Self::Latin1 => "Latin-1",
            Self::Windows1252 => "Windows-1252",
            Self::Cp437 => "CP437 (DOS)",
        }
    }

    /// Bytes per code unit: 2 for UTF-16, 1 otherwise. A line break is one unit.
    #[must_use]
    pub fn unit(self) -> usize {
        match self {
            Self::Utf16Le | Self::Utf16Be => 2,
            Self::Utf8 | Self::Latin1 | Self::Windows1252 | Self::Cp437 => 1,
        }
    }

    /// The byte-order mark of this encoding; empty for the single-byte ones.
    #[must_use]
    pub fn bom(self) -> &'static [u8] {
        match self {
            Self::Utf8 => b"\xEF\xBB\xBF",
            Self::Utf16Le => b"\xFF\xFE",
            Self::Utf16Be => b"\xFE\xFF",
            Self::Latin1 | Self::Windows1252 | Self::Cp437 => b"",
        }
    }

    /// The `encoding_rs` codec of this encoding; `None` for Latin-1 and CP437, which it lacks.
    #[must_use]
    pub(crate) fn codec(self) -> Option<&'static encoding_rs::Encoding> {
        match self {
            Self::Utf8 => Some(UTF_8),
            Self::Utf16Le => Some(UTF_16LE),
            Self::Utf16Be => Some(UTF_16BE),
            Self::Windows1252 => Some(WINDOWS_1252),
            Self::Latin1 | Self::Cp437 => None,
        }
    }

    /// The code unit `bytes` (one or two bytes, as [`TextEncoding::unit`] says) as a number.
    pub(crate) fn unit_value(self, bytes: &[u8]) -> u16 {
        match (self, bytes) {
            (Self::Utf16Le, [low, high, ..]) => u16::from_le_bytes([*low, *high]),
            (Self::Utf16Be, [high, low, ..]) => u16::from_be_bytes([*high, *low]),
            (_, [byte, ..]) => u16::from(*byte),
            (_, []) => 0,
        }
    }
}

/// An encoding as a file uses it: which one, and how long its byte-order mark is (0 without
/// one). Text starts after the mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detected {
    pub encoding: TextEncoding,
    pub bom_len: u64,
}

impl Detected {
    /// `encoding` chosen by hand for a file starting with `head`: its mark is skipped when the
    /// file has it.
    #[must_use]
    pub fn chosen(encoding: TextEncoding, head: &[u8]) -> Self {
        let bom = encoding.bom();
        let bom_len = if !bom.is_empty() && head.starts_with(bom) {
            bom.len() as u64
        } else {
            0
        };
        Self { encoding, bom_len }
    }
}

/// The encoding of a file whose first bytes are `head` (up to [`DETECT_BYTES`]). A head of
/// fewer bytes is the whole file; a full one may cut a UTF-8 character at its end.
#[must_use]
pub fn detect(head: &[u8]) -> Detected {
    detect_in(head, head.len() < DETECT_BYTES)
}

/// The encoding of a file whose bytes are all of `bytes`: a UTF-8 character cut off at the end
/// is not UTF-8.
#[must_use]
pub fn detect_whole(bytes: &[u8]) -> Detected {
    detect_in(bytes, true)
}

/// A whole file's bytes as text: detected by [`detect_whole`], the mark skipped, decoded by
/// [`decode`].
#[must_use]
pub fn decode_text(bytes: &[u8]) -> (Detected, String) {
    let detected = detect_whole(bytes);
    let skip = usize::try_from(detected.bom_len)
        .unwrap_or(usize::MAX)
        .min(bytes.len());
    (detected, decode(&bytes[skip..], detected.encoding))
}

/// The encoding of a DOS text file (`viewer.dos_extensions`) whose first bytes are `head`: as
/// [`detect`], but CP437 where that answers Windows-1252 or the head is plain ASCII. A mark,
/// UTF-16 and UTF-8 with characters beyond ASCII keep their own: an `.nfo` written today may be
/// UTF-8, and box-drawing art in CP437 is hardly ever valid UTF-8.
#[must_use]
pub fn detect_dos(head: &[u8]) -> Detected {
    let detected = detect(head);
    let ascii = detected.encoding == TextEncoding::Utf8 && detected.bom_len == 0 && head.is_ascii();
    if ascii || detected.encoding == TextEncoding::Windows1252 {
        Detected::chosen(TextEncoding::Cp437, head)
    } else {
        detected
    }
}

fn detect_in(head: &[u8], complete: bool) -> Detected {
    for encoding in [
        TextEncoding::Utf8,
        TextEncoding::Utf16Le,
        TextEncoding::Utf16Be,
    ] {
        if head.starts_with(encoding.bom()) {
            return Detected::chosen(encoding, head);
        }
    }
    let encoding = utf16_without_bom(head).unwrap_or(if is_utf8(head, complete) {
        TextEncoding::Utf8
    } else {
        TextEncoding::Windows1252
    });
    Detected {
        encoding,
        bom_len: 0,
    }
}

/// The code units a head needs before it can pass for UTF-16 without a mark.
const MIN_UTF16_UNITS: usize = 4;

/// UTF-16 without a mark: at least 40 % of the code units have a zero high byte and almost none
/// a zero low byte - the shape of mostly-ASCII text, which `wsl.exe` and PowerShell write. Fewer
/// than [`MIN_UTF16_UNITS`] units are too few to tell (an `.exe` header starts `MZ\x90\0`).
fn utf16_without_bom(head: &[u8]) -> Option<TextEncoding> {
    let units = head.len() / 2;
    if units < MIN_UTF16_UNITS {
        return None;
    }
    let (mut first_zero, mut second_zero) = (0usize, 0usize);
    for unit in head.as_chunks::<2>().0 {
        first_zero += usize::from(unit[0] == 0);
        second_zero += usize::from(unit[1] == 0);
    }
    let many = |count: usize| count * 10 >= units * 4;
    let few = |count: usize| count * 20 < units;
    if many(second_zero) && few(first_zero) {
        Some(TextEncoding::Utf16Le)
    } else if many(first_zero) && few(second_zero) {
        Some(TextEncoding::Utf16Be)
    } else {
        None
    }
}

/// Valid UTF-8; unless `complete`, a character cut off at the end of the head is allowed.
fn is_utf8(head: &[u8], complete: bool) -> bool {
    match std::str::from_utf8(head) {
        Ok(_) => true,
        Err(error) => !complete && error.error_len().is_none(),
    }
}

/// Text decoded from a window of a file, and the bytes it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub text: String,
    /// The offset of the first byte decoded.
    pub start: u64,
    /// The offset after the last byte decoded: where the next window continues.
    pub end: u64,
}

/// Decodes `bytes`, which lie at `offset` in the file. The byte-order mark is never text; a
/// window starting inside a character skips the rest of it, one ending inside a character
/// stops before it - unless `at_eof`, where a cut character becomes U+FFFD.
#[must_use]
pub fn decode_window(bytes: &[u8], offset: u64, detected: Detected, at_eof: bool) -> Decoded {
    let (start, end) = char_bounds(bytes, offset, detected, at_eof);
    Decoded {
        text: decode(&bytes[start..end], detected.encoding),
        start: offset + start as u64,
        end: offset + end as u64,
    }
}

/// The part `start..end` of `bytes` (at `offset`) that holds whole characters.
pub(crate) fn char_bounds(
    bytes: &[u8],
    offset: u64,
    detected: Detected,
    at_eof: bool,
) -> (usize, usize) {
    let len = bytes.len();
    let mut start = usize::try_from(detected.bom_len.saturating_sub(offset))
        .unwrap_or(len)
        .min(len);
    let mut end = len;
    match detected.encoding {
        TextEncoding::Utf8 => {
            let mut skipped = 0;
            while offset + (start as u64) > detected.bom_len
                && start < len
                && skipped < 3
                && is_continuation(bytes[start])
            {
                start += 1;
                skipped += 1;
            }
            if !at_eof {
                end -= incomplete_utf8_tail(&bytes[start..]);
            }
        }
        TextEncoding::Utf16Le | TextEncoding::Utf16Be => {
            let encoding = detected.encoding;
            if (offset + start as u64).saturating_sub(detected.bom_len) % 2 == 1 {
                start = (start + 1).min(len);
            }
            if len - start >= 2 && is_low_surrogate(encoding.unit_value(&bytes[start..])) {
                start += 2;
            }
            if !at_eof {
                end = start + (len - start) / 2 * 2;
                if end - start >= 2 && is_high_surrogate(encoding.unit_value(&bytes[end - 2..])) {
                    end -= 2;
                }
            }
        }
        TextEncoding::Latin1 | TextEncoding::Windows1252 | TextEncoding::Cp437 => {}
    }
    (start, end.max(start))
}

fn is_continuation(byte: u8) -> bool {
    byte & 0xC0 == 0x80
}

fn is_high_surrogate(unit: u16) -> bool {
    (0xD800..0xDC00).contains(&unit)
}

fn is_low_surrogate(unit: u16) -> bool {
    (0xDC00..0xE000).contains(&unit)
}

/// How many bytes at the end of `bytes` are the start of a character that goes on after them.
fn incomplete_utf8_tail(bytes: &[u8]) -> usize {
    let len = bytes.len();
    for back in 1..=len.min(4) {
        let byte = bytes[len - back];
        if is_continuation(byte) {
            continue;
        }
        let wanted = match byte {
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF7 => 4,
            _ => 1,
        };
        return if wanted > back { back } else { 0 };
    }
    0
}

/// `bytes` as text; what does not decode becomes U+FFFD. The text is reserved at its worst-case
/// size before it is written, as `encoding_rs` does, so growing it rarely leaves a copy of it
/// (a password in an INI file) behind: two bytes a character for Latin-1, three for CP437's box
/// drawing.
#[must_use]
pub fn decode(bytes: &[u8], encoding: TextEncoding) -> String {
    if let Some(codec) = encoding.codec() {
        return codec.decode_without_bom_handling(bytes).0.into_owned();
    }
    let dos = encoding == TextEncoding::Cp437;
    let of_byte: fn(u8) -> char = if dos { cp437::decode_byte } else { char::from };
    let mut text = String::with_capacity(bytes.len() * if dos { 3 } else { 2 });
    text.extend(bytes.iter().map(|&byte| of_byte(byte)));
    text
}

/// `c` in `encoding`; `None` when the encoding has no such character.
#[must_use]
pub fn encode_char(c: char, encoding: TextEncoding) -> Option<Vec<u8>> {
    match encoding {
        TextEncoding::Utf8 => Some(c.encode_utf8(&mut [0; 4]).as_bytes().to_vec()),
        TextEncoding::Utf16Le | TextEncoding::Utf16Be => {
            let mut units = [0; 2];
            let units = c.encode_utf16(&mut units);
            Some(
                units
                    .iter()
                    .flat_map(|unit| match encoding {
                        TextEncoding::Utf16Be => unit.to_be_bytes(),
                        _ => unit.to_le_bytes(),
                    })
                    .collect(),
            )
        }
        TextEncoding::Latin1 => u8::try_from(u32::from(c)).ok().map(|byte| vec![byte]),
        TextEncoding::Cp437 => cp437::encode_char(c).map(|byte| vec![byte]),
        TextEncoding::Windows1252 => {
            let mut utf8 = [0; 4];
            let (bytes, _, unmappable) = WINDOWS_1252.encode(c.encode_utf8(&mut utf8));
            (!unmappable).then(|| bytes.into_owned())
        }
    }
}

#[cfg(test)]
#[path = "encoding_tests.rs"]
mod tests;
