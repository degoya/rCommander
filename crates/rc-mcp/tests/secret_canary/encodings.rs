//! The forms a canary could leak in, and the search for them (RC-080-61): plain, hex in both
//! cases, base64 from each of the three offsets a value can start at in an encoded stream,
//! URL-encoded with either case of hex digit, and UTF-16 in both byte orders. Each form has its
//! counter-check below: a canary hidden in that form, inside other text, is found.

use std::borrow::Cow;

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// What the sweep looks for: every form of every canary, each once.
#[derive(Clone, Default)]
pub struct Needles(Vec<String>);

impl Needles {
    /// Every form of each of `canaries`.
    pub fn of<S: AsRef<str>>(canaries: &[S]) -> Self {
        let mut needles: Vec<String> = canaries
            .iter()
            .flat_map(|canary| forms(canary.as_ref()))
            .collect();
        needles.sort();
        needles.dedup();
        Self(needles)
    }

    /// Both sets in one.
    pub fn with(mut self, other: &Self) -> Self {
        self.0.extend(other.0.iter().cloned());
        self.0.sort();
        self.0.dedup();
        self
    }

    /// The needles `haystack` carries. Every needle is ASCII (UTF-16 adds NUL bytes, ASCII too),
    /// and a lossy decode never changes an ASCII byte, so searching the decoded text finds every
    /// needle the bytes hold - with the standard library's substring search, fast enough for
    /// megabytes of log against hundreds of needles.
    pub fn found(&self, haystack: &[u8]) -> Vec<String> {
        let text: Cow<'_, str> = String::from_utf8_lossy(haystack);
        self.0
            .iter()
            .filter(|needle| text.contains(needle.as_str()))
            .cloned()
            .collect()
    }

    /// How many forms are looked for.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

/// Every form `canary` could leak in.
pub fn forms(canary: &str) -> Vec<String> {
    let bytes = canary.as_bytes();
    let mut forms = vec![canary.to_owned(), hex(bytes, false), hex(bytes, true)];
    // A value at stream offset 0, 1 or 2 (mod 3): the bytes from the next group boundary on are
    // encoded as if they stood alone.
    for skip in 0..3 {
        forms.push(base64_groups(&bytes[skip.min(bytes.len())..]));
    }
    forms.push(url_encoded(bytes, false));
    forms.push(url_encoded(bytes, true));
    forms.push(utf16(canary, false));
    forms.push(utf16(canary, true));
    forms.retain(|form| !form.is_empty());
    forms
}

/// `bytes` as hex digits.
pub fn hex(bytes: &[u8], upper: bool) -> String {
    bytes
        .iter()
        .map(|byte| {
            if upper {
                format!("{byte:02X}")
            } else {
                format!("{byte:02x}")
            }
        })
        .collect()
}

/// `bytes` as standard base64, full groups of three only: what the encoding of a longer stream
/// holds of them wherever they start on a group boundary. The chars of a last, partial group
/// depend on what follows in the stream, so they are left out.
pub fn base64_groups(bytes: &[u8]) -> String {
    let mut text = String::new();
    for [first, second, third] in bytes.as_chunks::<3>().0 {
        let joined =
            (usize::from(*first) << 16) | (usize::from(*second) << 8) | usize::from(*third);
        for shift in [18, 12, 6, 0] {
            text.push(char::from(BASE64[(joined >> shift) & 0x3f]));
        }
    }
    text
}

/// `bytes` percent-encoded as RFC 3986 does it: the unreserved characters stay, every other
/// byte becomes `%XX`.
pub fn url_encoded(bytes: &[u8], upper: bool) -> String {
    bytes
        .iter()
        .map(|&byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                char::from(byte).to_string()
            } else {
                format!("%{}", hex(&[byte], upper))
            }
        })
        .collect()
}

/// `text` in UTF-16, little or big endian, as bytes; for the ASCII canaries this is ASCII and
/// NUL bytes.
pub fn utf16(text: &str, big_endian: bool) -> String {
    let bytes: Vec<u8> = text
        .encode_utf16()
        .flat_map(|unit| {
            if big_endian {
                unit.to_be_bytes()
            } else {
                unit.to_le_bytes()
            }
        })
        .collect();
    String::from_utf8(bytes).unwrap_or_default()
}

#[cfg(test)]
#[path = "encodings_tests.rs"]
mod tests;
