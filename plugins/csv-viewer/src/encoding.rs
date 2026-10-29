//! The file's bytes as the UTF-8 text the host gets: as they are (with U+FFFD for what is not
//! UTF-8), or read as Windows-1252 - both decoded by `rc-plugin-text` (RC-100-22).
//!
//! Every byte CSV's syntax uses is ASCII in both encodings and stays where it is relative to its
//! neighbours, so the fields found in the file's bytes are the fields of the text.

// How the file's bytes are read, once `auto` is decided: `Utf8` or `Windows1252`.
use rc_plugin_text::Decoder;
pub use rc_plugin_text::Encoding;

/// Appends `bytes` read as `encoding` to `out` as UTF-8. UTF-8 that is not valid gets U+FFFD as
/// [`String::from_utf8_lossy`] gives it; whole lines and whole files decode alike, since a line
/// break never sits inside a character.
pub fn decode_into(bytes: &[u8], encoding: Encoding, out: &mut Vec<u8>) {
    out.extend_from_slice(decode(bytes, encoding).as_bytes());
}

/// `bytes` read as `encoding`, as a string.
#[must_use]
pub fn decode(bytes: &[u8], encoding: Encoding) -> String {
    let mut text = String::with_capacity(bytes.len());
    let mut decoder = Decoder::new(encoding);
    decoder.push(bytes, &mut text);
    decoder.finish(&mut text);
    text
}

/// Whether a stream is valid UTF-8, one byte at a time (the Unicode Standard's table 3-7): no
/// overlong form, no surrogate, nothing above U+10FFFF, no character cut at the end.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Utf8Check {
    need: u8,
    lower: u8,
    upper: u8,
    valid: bool,
}

impl Default for Utf8Check {
    fn default() -> Self {
        Self {
            need: 0,
            lower: 0x80,
            upper: 0xBF,
            valid: true,
        }
    }
}

impl Utf8Check {
    pub(crate) fn feed(&mut self, byte: u8) {
        if !self.valid || (self.need == 0 && byte.is_ascii()) {
            return;
        }
        if self.need > 0 {
            if !(self.lower..=self.upper).contains(&byte) {
                self.valid = false;
                return;
            }
            self.need -= 1;
            (self.lower, self.upper) = (0x80, 0xBF);
            return;
        }
        (self.need, self.lower, self.upper) = match byte {
            0xC2..=0xDF => (1, 0x80, 0xBF),
            0xE0 => (2, 0xA0, 0xBF),
            0xE1..=0xEC | 0xEE..=0xEF => (2, 0x80, 0xBF),
            0xED => (2, 0x80, 0x9F),
            0xF0 => (3, 0x90, 0xBF),
            0xF1..=0xF3 => (3, 0x80, 0xBF),
            0xF4 => (3, 0x80, 0x8F),
            _ => {
                self.valid = false;
                return;
            }
        };
    }

    /// Whether everything fed was valid UTF-8 and ended on a character's end.
    pub(crate) fn valid(&self) -> bool {
        self.valid && self.need == 0
    }
}

#[cfg(test)]
#[path = "encoding_tests.rs"]
mod tests;
