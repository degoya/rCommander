//! Keys named by where they sit (RC-100-48): `[Semicolon]`, the W3C `KeyboardEvent.code` in
//! brackets, is the key right of L whatever the layout prints on it - O-umlaut on a German
//! keyboard, `;` on a US one, n-tilde on a Spanish one. A chord on such a key follows the key
//! across layouts, also after a switch while the program runs (owner 2026-10-29). Only the keys
//! of the main block whose character depends on the layout have a name by place: punctuation and
//! umlauts. Letters and digits stay characters, as Total Commander binds them, and the key
//! `Backslash` is always `\` already (`rcommander/src/keymap.rs`).
//!
//! The window tells the code of a pressed key in [`Chord::physical`]; a binding written `[Code]`
//! matches it ([`Chord::matches`]), a binding written with the character matches the character
//! as before and wins when both are bound ([`Keymap::lookup`](super::Keymap::lookup)). A person
//! reads a key named by place as the active layout prints it ([`Chord::label`]), else as a US
//! keyboard does.

use std::hash::{Hash, Hasher};

use super::Chord;

/// The keys a chord can name by place, with what a US keyboard prints on them.
pub const PHYSICAL_KEYS: &[(&str, &str)] = &[
    ("Backquote", "`"),
    ("Minus", "-"),
    ("Equal", "="),
    ("BracketLeft", "["),
    ("BracketRight", "]"),
    ("Semicolon", ";"),
    ("Quote", "'"),
    ("Comma", ","),
    ("Period", "."),
    ("Slash", "/"),
];

/// `code` as [`PHYSICAL_KEYS`] holds it, when a chord can name that key by place.
#[must_use]
pub fn physical_code(code: &str) -> Option<&'static str> {
    PHYSICAL_KEYS
        .iter()
        .map(|(known, _)| *known)
        .find(|known| *known == code)
}

/// The code of a key name written by place, `Semicolon` for `[Semicolon]`.
pub(super) fn bracketed(key: &str) -> Option<&'static str> {
    physical_code(key.strip_prefix('[')?.strip_suffix(']')?)
}

/// What a US keyboard prints on the key `code`.
fn us(code: &str) -> &'static str {
    PHYSICAL_KEYS
        .iter()
        .find(|(known, _)| *known == code)
        .map_or("?", |(_, character)| character)
}

impl Chord {
    /// The code of the key this chord names by place: `Semicolon` for `Ctrl+[Semicolon]`; none
    /// for a key named by its character or its name.
    #[must_use]
    pub fn code(&self) -> Option<&'static str> {
        bracketed(&self.key)
    }

    /// Whether the key press `pressed` runs this chord: the same chord, or this chord names by
    /// place the key `pressed` sat on, with the same modifiers.
    #[must_use]
    pub fn matches(&self, pressed: &Self) -> bool {
        *self == *pressed
            || (self.code().is_some()
                && self.code() == pressed.physical
                && (self.ctrl, self.shift, self.alt) == (pressed.ctrl, pressed.shift, pressed.alt))
    }

    /// The key press `self` as the keymap editor stores it: by place when the layout decides its
    /// character and that is no ASCII letter or digit - an umlaut, a punctuation mark, a dead
    /// key -, else as it is.
    #[must_use]
    pub fn stored(&self) -> Self {
        let mut chars = self.key.chars();
        let letter =
            matches!((chars.next(), chars.next()), (Some(c), None) if c.is_ascii_alphanumeric());
        match self.physical {
            Some(code) if !letter => Self {
                key: format!("[{code}]"),
                ..self.clone()
            },
            _ => self.clone(),
        }
    }

    /// The chord as a person reads it, as [`Display`](std::fmt::Display) prints it, but a key
    /// named by place as `character` says the active layout prints it - Ctrl+O-umlaut on a German
    /// layout, a dead key's accent as it is (Ctrl+acute on a Spanish one) - and as a US keyboard
    /// does where `character` knows none.
    #[must_use]
    pub fn label(&self, character: impl Fn(&str) -> Option<char>) -> String {
        let Some(code) = self.code() else {
            return self.to_string();
        };
        let shown = character(code).filter(|c| !c.is_whitespace() && !c.is_control());
        let key = match shown {
            // `+` joins a chord's parts; the key is `Plus`, as the keymap writes it.
            Some('+') => "Plus".to_owned(),
            Some(c) => c.to_lowercase().collect(),
            None => us(code).to_owned(),
        };
        Self {
            key,
            physical: None,
            ..self.clone()
        }
        .to_string()
    }
}

/// A chord is its key and its modifiers: the code a key press carries is no part of it.
impl PartialEq for Chord {
    fn eq(&self, other: &Self) -> bool {
        (&self.key, self.ctrl, self.shift, self.alt)
            == (&other.key, other.ctrl, other.shift, other.alt)
    }
}

impl Eq for Chord {}

impl Hash for Chord {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (&self.key, self.ctrl, self.shift, self.alt).hash(state);
    }
}
