//! What `wsl.exe` prints, as text and as distribution names.
//!
//! `wsl.exe` writes UTF-16LE even into a pipe - with a byte order mark on some versions, padded
//! with NUL characters on others, with CRLF or LF line ends. Read as UTF-8, every name carries a
//! NUL byte between its letters and no longer works as a path. Versions since 0.64 write UTF-8
//! when `WSL_UTF8=1` is set, which the caller does; both forms are read here.

use super::details::Details;
use crate::paths::valid_distro;

const UTF16LE_BOM: [u8; 2] = [0xFF, 0xFE];
const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// The text of `bytes`: UTF-16LE with or without its byte order mark, otherwise UTF-8, invalid
/// sequences replaced; NUL characters removed.
///
/// Deliberately not the engine's file decoding (`rc_engine::viewer::decode_text`, RC-080-21,
/// which this crate cannot reach): `wsl.exe` output is never ANSI and never UTF-16 BE, and its
/// short messages with a few non-ASCII characters need the looser UTF-16 test below.
pub(crate) fn decode(bytes: &[u8]) -> String {
    let text = if let Some(rest) = bytes.strip_prefix(&UTF16LE_BOM) {
        utf16le(rest)
    } else if let Some(rest) = bytes.strip_prefix(&UTF8_BOM) {
        String::from_utf8_lossy(rest).into_owned()
    } else if looks_utf16le(bytes) {
        utf16le(bytes)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    };
    text.replace('\0', "")
}

/// ASCII in UTF-16LE has a zero in every second byte; UTF-8 text has none at all. Half of the
/// odd bytes zero leaves room for a few characters past ASCII in a message.
fn looks_utf16le(bytes: &[u8]) -> bool {
    let odd = bytes.iter().skip(1).step_by(2);
    let total = odd.clone().count();
    let zeros = odd.filter(|byte| **byte == 0).count();
    total > 0 && zeros * 2 >= total
}

/// UTF-16LE code units to text; a trailing odd byte is dropped.
fn utf16le(bytes: &[u8]) -> String {
    let units = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair));
    char::decode_utf16(units)
        .map(|unit| unit.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

/// The names in the output of `wsl.exe --list --quiet`, in its order, each once.
///
/// A line that is not a distribution name is skipped: blank lines, and every line of the
/// messages `wsl.exe` prints instead of a list (WSL missing or switched off, no distribution
/// installed), which hold spaces or punctuation a name cannot.
pub(crate) fn names(text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for line in text.lines() {
        let name = line.trim();
        if is_distro_name(name) && !names.iter().any(|known| known.eq_ignore_ascii_case(name)) {
            names.push(name.to_owned());
        }
    }
    names
}

/// The default and the versions from the output of `wsl.exe --list --verbose`:
///
/// ```text
///   NAME            STATE           VERSION
/// * Ubuntu          Running         2
///   Debian          Stopped         1
/// ```
///
/// The header and the state are translated, and a translated state may hold a space, so a row
/// is read from its ends: the marker and the name in front, the version last. The header is the
/// line whose last word is no version.
pub(crate) fn verbose(text: &str) -> Details {
    let mut details = Details::default();
    for line in text.lines() {
        let line = line.trim();
        let (is_default, row) = match line.strip_prefix('*') {
            Some(row) => (true, row),
            None => (false, line),
        };
        let mut words = row.split_whitespace();
        let (Some(name), Some(last)) = (words.next(), words.next_back()) else {
            continue;
        };
        let Ok(version @ 1..=2) = last.parse::<u8>() else {
            continue;
        };
        if !is_distro_name(name) {
            continue;
        }
        if is_default {
            details.default = Some(name.to_owned());
        }
        details.versions.push((name.to_owned(), version));
    }
    details
}

/// What WSL accepts as a distribution name: ASCII letters, digits, `.`, `-` and `_`.
fn is_distro_name(name: &str) -> bool {
    valid_distro(name)
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

#[cfg(test)]
#[path = "decode_tests.rs"]
mod tests;
