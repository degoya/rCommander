//! The character the active keyboard layout prints on a key named by where it sits (RC-100-48):
//! the menus, the palette and the settings show `Ctrl+[Semicolon]` as Ctrl+O-umlaut on a German
//! layout and as Ctrl+; on a US one. Windows asks the layout of the window's thread
//! (`windows/key_label.rs`), so a layout switched while the program runs shows at the next look;
//! elsewhere there is none for now, and `rc_engine::Chord::label` shows the US character.

/// The scan code (set 1) of each key a chord can name by place (`rc_engine::PHYSICAL_KEYS`).
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read only by the Windows half")
)]
pub(super) const SCAN_CODES: &[(&str, u32)] = &[
    ("Backquote", 0x29),
    ("Minus", 0x0C),
    ("Equal", 0x0D),
    ("BracketLeft", 0x1A),
    ("BracketRight", 0x1B),
    ("Semicolon", 0x27),
    ("Quote", 0x28),
    ("Comma", 0x33),
    ("Period", 0x34),
    ("Slash", 0x35),
];

/// What the active layout prints on the key `code` without modifiers - a dead key's accent
/// too -; none where the system does not say.
pub fn character(code: &str) -> Option<char> {
    #[cfg(windows)]
    {
        let scan = SCAN_CODES.iter().find(|(known, _)| *known == code)?.1;
        super::windows::key_label::character(scan)
    }
    #[cfg(not(windows))]
    {
        let _ = code;
        None
    }
}

#[cfg(test)]
#[path = "key_label_tests.rs"]
mod tests;
