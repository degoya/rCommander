//! [`Decoder`]: a file's bytes as UTF-8 text, window by window.

use crate::{Encoding, tables};

/// What an invalid sequence becomes.
const REPLACEMENT: char = '\u{FFFD}';

/// Decodes a file window by window into UTF-8.
///
/// A UTF-8 character, a UTF-16 unit or a surrogate pair that a window's end cuts waits for the
/// next window, so the text is the same wherever the windows fall. What is invalid becomes
/// U+FFFD: in UTF-8 one for each maximal invalid part, as `String::from_utf8_lossy` writes it; in
/// UTF-16 one for each unpaired surrogate, as `String::from_utf16_lossy` does, and one for an odd
/// byte at the end. A single-byte code page maps every byte; a line break is ASCII in each, so no
/// character spans one.
#[derive(Clone, Debug)]
pub struct Decoder {
    encoding: Encoding,
    /// UTF-8: the start of a character the last window cut. UTF-16: the odd byte it left.
    carry: Vec<u8>,
    /// UTF-16: a high surrogate waiting for its low half.
    high: Option<u16>,
}

impl Decoder {
    /// A decoder from `encoding`, at the start of a file.
    #[must_use]
    pub fn new(encoding: Encoding) -> Self {
        Self {
            encoding,
            carry: Vec::new(),
            high: None,
        }
    }

    /// Appends the text of the next window, `bytes`, to `out`; a character cut at its end waits
    /// for the next window or [`Decoder::finish`].
    pub fn push(&mut self, bytes: &[u8], out: &mut String) {
        match self.encoding {
            Encoding::Utf8 => self.utf8(bytes, out),
            Encoding::Utf16Le => self.utf16(bytes, out, u16::from_le_bytes),
            Encoding::Utf16Be => self.utf16(bytes, out, u16::from_be_bytes),
            Encoding::Windows1252 => single_byte(bytes, &tables::WINDOWS_1252, out),
            Encoding::Cp850 => single_byte(bytes, &tables::CP850, out),
            Encoding::Cp437 => single_byte(bytes, &tables::CP437, out),
        }
    }

    /// The end of the file: a character still cut is invalid. The decoder is then at the start
    /// of a file again.
    pub fn finish(&mut self, out: &mut String) {
        if self.high.take().is_some() {
            out.push(REPLACEMENT);
        }
        if !self.carry.is_empty() {
            self.carry.clear();
            out.push(REPLACEMENT);
        }
    }

    fn utf8(&mut self, bytes: &[u8], out: &mut String) {
        let mut rest = self.utf8_carried(bytes, out);
        loop {
            match std::str::from_utf8(rest) {
                Ok(text) => {
                    out.push_str(text);
                    return;
                }
                Err(error) => {
                    let (valid, after) = rest.split_at(error.valid_up_to());
                    // `valid_up_to` is where the valid prefix ends, so this cannot fail.
                    out.push_str(std::str::from_utf8(valid).unwrap_or_default());
                    match error.error_len() {
                        Some(length) => {
                            out.push(REPLACEMENT);
                            rest = after.get(length..).unwrap_or_default();
                        }
                        None => {
                            self.carry = after.to_vec();
                            return;
                        }
                    }
                }
            }
        }
    }

    /// Puts the character the last window cut together with the first bytes of `bytes` - at
    /// most three of them, never the whole window; the bytes after it.
    fn utf8_carried<'a>(&mut self, bytes: &'a [u8], out: &mut String) -> &'a [u8] {
        if self.carry.is_empty() {
            return bytes;
        }
        let before = self.carry.len();
        let mut head = std::mem::take(&mut self.carry);
        let taken = bytes.len().min(4_usize.saturating_sub(before));
        head.extend_from_slice(&bytes[..taken]);
        let used = match std::str::from_utf8(&head) {
            Ok(text) => {
                out.push_str(text);
                taken
            }
            // The cut character is whole: what follows it is the window's own.
            Err(error) if error.valid_up_to() > 0 => {
                let valid = error.valid_up_to();
                out.push_str(std::str::from_utf8(&head[..valid]).unwrap_or_default());
                valid.saturating_sub(before)
            }
            Err(error) => match error.error_len() {
                Some(length) => {
                    out.push(REPLACEMENT);
                    length.saturating_sub(before)
                }
                // Still cut: the window was shorter than the character's rest.
                None => {
                    self.carry = head;
                    return &[];
                }
            },
        };
        bytes.get(used..).unwrap_or_default()
    }

    fn utf16(&mut self, bytes: &[u8], out: &mut String, unit: fn([u8; 2]) -> u16) {
        let mut rest = bytes;
        if let Some(&odd) = self.carry.first() {
            let Some((&next, after)) = rest.split_first() else {
                return;
            };
            self.carry.clear();
            self.push_unit(unit([odd, next]), out);
            rest = after;
        }
        out.reserve(rest.len() / 2);
        let (pairs, odd) = rest.as_chunks::<2>();
        for &pair in pairs {
            self.push_unit(unit(pair), out);
        }
        self.carry.extend_from_slice(odd);
    }

    fn push_unit(&mut self, unit: u16, out: &mut String) {
        if let Some(high) = self.high.take() {
            if (0xDC00..=0xDFFF).contains(&unit) {
                let code =
                    0x1_0000 + ((u32::from(high) - 0xD800) << 10) + (u32::from(unit) - 0xDC00);
                out.push(char::from_u32(code).unwrap_or(REPLACEMENT));
                return;
            }
            out.push(REPLACEMENT);
        }
        match unit {
            0xD800..=0xDBFF => self.high = Some(unit),
            0xDC00..=0xDFFF => out.push(REPLACEMENT),
            _ => out.push(char::from_u32(u32::from(unit)).unwrap_or(REPLACEMENT)),
        }
    }
}

/// A code page whose bytes from 0x80 are `table` and below are ASCII.
fn single_byte(bytes: &[u8], table: &[char; 128], out: &mut String) {
    out.reserve(bytes.len());
    for byte in bytes {
        match byte.checked_sub(0x80) {
            Some(high) => out.push(table[usize::from(high)]),
            None => out.push(char::from(*byte)),
        }
    }
}

#[cfg(test)]
#[path = "decoder_tests.rs"]
mod tests;
