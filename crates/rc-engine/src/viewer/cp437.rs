//! Code page 437, the character set of the IBM PC and DOS (RC-020-11): what `.nfo` and `.diz`
//! files and their box-drawing art are written in. `encoding_rs` has no IBM437, so the high half
//! is the table below; the low half is ASCII.
//!
//! CP437's pictures for 0x01-0x1F and 0x7F (smileys, card suits, arrows, the house) are not
//! used: a DOS text file's line breaks, tabs and its end-of-file mark 0x1A are controls there
//! too, and the viewer shows a control as a dot, as it does in every other encoding. Which files
//! open in CP437 is the setting `viewer.dos_extensions`.

use std::path::Path;

use rc_db::Db;

use crate::settings::{DOS_EXTENSIONS_DEFAULT, DOS_EXTENSIONS_KEY, stored_setting};

/// The characters of the bytes 0x80 to 0xFF.
const HIGH: [char; 128] = [
    // 0x80
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}', '\u{00E7}',
    // 0x88
    '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}', '\u{00EE}', '\u{00EC}', '\u{00C4}', '\u{00C5}',
    // 0x90
    '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}', '\u{00F2}', '\u{00FB}', '\u{00F9}',
    // 0x98
    '\u{00FF}', '\u{00D6}', '\u{00DC}', '\u{00A2}', '\u{00A3}', '\u{00A5}', '\u{20A7}', '\u{0192}',
    // 0xA0
    '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}', '\u{00F1}', '\u{00D1}', '\u{00AA}', '\u{00BA}',
    // 0xA8
    '\u{00BF}', '\u{2310}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00BB}',
    // 0xB0
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}',
    // 0xB8
    '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255D}', '\u{255C}', '\u{255B}', '\u{2510}',
    // 0xC0
    '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}', '\u{255F}',
    // 0xC8
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{2567}',
    // 0xD0
    '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}', '\u{2558}', '\u{2552}', '\u{2553}', '\u{256B}',
    // 0xD8
    '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}', '\u{2580}',
    // 0xE0
    '\u{03B1}', '\u{00DF}', '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}', '\u{03C4}',
    // 0xE8
    '\u{03A6}', '\u{0398}', '\u{03A9}', '\u{03B4}', '\u{221E}', '\u{03C6}', '\u{03B5}', '\u{2229}',
    // 0xF0
    '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}', '\u{2248}',
    // 0xF8
    '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}', '\u{25A0}', '\u{00A0}',
];

/// The character of `byte`.
#[must_use]
pub(crate) fn decode_byte(byte: u8) -> char {
    match byte.checked_sub(0x80) {
        Some(high) => HIGH[usize::from(high)],
        None => char::from(byte),
    }
}

/// The byte of `c`; `None` when CP437 has no such character.
#[must_use]
pub(crate) fn encode_char(c: char) -> Option<u8> {
    if c.is_ascii() {
        return u8::try_from(c).ok();
    }
    HIGH.iter()
        .position(|&high| high == c)
        .and_then(|at| u8::try_from(0x80 + at).ok())
}

/// Whether `path` opens in CP437: its extension is one of the words of `extensions` (the
/// setting's value), in any case.
#[must_use]
pub fn is_dos_file(path: &Path, extensions: &str) -> bool {
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return false;
    };
    extensions
        .split_whitespace()
        .any(|word| word.eq_ignore_ascii_case(extension))
}

/// `viewer.dos_extensions`; its default without a database.
pub async fn dos_extensions(db: Option<&Db>) -> String {
    let stored = match db {
        Some(db) => stored_setting(db, DOS_EXTENSIONS_KEY).await,
        None => None,
    };
    stored
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| DOS_EXTENSIONS_DEFAULT.to_owned())
}

#[cfg(test)]
#[path = "cp437_tests.rs"]
mod tests;
