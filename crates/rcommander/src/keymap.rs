//! Key presses spelled as the engine's keymap reads them.
//!
//! The window holds no bindings: [`chord`] turns a key press into an [`rc_engine::Chord`] and
//! [`rc_engine::Keymap::lookup`] names the command. A named key keeps iced's name, the W3C one
//! (`Enter`, `PageDown`); a character key is the character the layout gives without modifiers,
//! in lower case, also with Shift held (iced's key without modifiers). A key of the main block
//! whose character the layout decides also tells where it sits ([`Chord::physical`], RC-100-48),
//! so a binding by place finds it on every layout: the terminal toggle, `Ctrl+[Semicolon]`, is
//! Ctrl+O-umlaut on a German keyboard and Ctrl+; on a US one, also after a switch of the layout
//! while the window runs. A dead key with Ctrl or Alt is a chord on its place alone - the Spanish
//! acute right of n-tilde, `Ctrl+[Quote]`; without them it starts an accented letter. Some keys
//! are named by where they sit rather than by what the layout prints: the key `Backslash` is
//! always `\`, so Ctrl+\ is Ctrl+# on a German keyboard, as Total Commander documents it; and the
//! keypad's grey keys are `NumAdd`, `NumSubtract`, `NumMultiply` and `NumDivide`, apart from `+`,
//! `-`, `*` and `/` elsewhere; the `+` of the main block is `Plus`, as `+` joins a chord's parts
//! (RC-020-12). AltGr with a letter it types nothing for is Ctrl+Alt with it ([`pressed`]).
//! A person reads a key named by place as the active layout prints it ([`label`]).

use iced::keyboard::{
    Key, Modifiers,
    key::{Code, Physical},
};
use rc_engine::Chord;

use crate::platform::key_label;

/// The chord of a key press; `None` for a key no chord can name (a modifier on its own) and for
/// any press with the Windows key, which belongs to the system.
pub fn chord(key: &Key, physical: Physical, modifiers: Modifiers) -> Option<Chord> {
    if modifiers.logo() {
        return None;
    }
    let code = match physical {
        Physical::Code(code) => rc_engine::physical_code(&format!("{code:?}")),
        Physical::Unidentified(_) => None,
    };
    let name = match (key, physical) {
        (_, Physical::Code(Code::Backslash)) => "\\".to_owned(),
        (_, Physical::Code(Code::NumpadAdd)) => "NumAdd".to_owned(),
        (_, Physical::Code(Code::NumpadSubtract)) => "NumSubtract".to_owned(),
        (_, Physical::Code(Code::NumpadMultiply)) => "NumMultiply".to_owned(),
        (_, Physical::Code(Code::NumpadDivide)) => "NumDivide".to_owned(),
        (Key::Named(named), _) => format!("{named:?}"),
        (Key::Character(text), _) if text.as_str() == "+" => "Plus".to_owned(),
        (Key::Character(text), _) => text.to_string(),
        // A dead key: no character, but a place.
        (Key::Unidentified, _) if modifiers.control() || modifiers.alt() => format!("[{}]", code?),
        _ => return None,
    };
    let mut chord = Chord::plain(&name)?;
    chord.ctrl = modifiers.control();
    chord.shift = modifiers.shift();
    chord.alt = modifiers.alt();
    chord.physical = code;
    Some(chord)
}

/// `chord` as a person reads it: a key named by place as the active keyboard layout prints it,
/// else as a US keyboard does.
pub fn label(chord: &Chord) -> String {
    chord.label(key_label::character)
}

/// What the active layout prints on every key a chord can name by place: it changes with the
/// layout, so the menus know when to show their keys again.
pub fn layout() -> String {
    rc_engine::PHYSICAL_KEYS
        .iter()
        .map(|(code, _)| key_label::character(code).unwrap_or(' '))
        .collect()
}

/// `keys`, a chord as the keymap writes it, as [`label`] shows it; the text as it is when it is
/// no chord.
pub fn label_text(keys: &str) -> String {
    keys.parse::<Chord>()
        .map_or_else(|_| keys.to_owned(), |chord| label(&chord))
}

/// The chord of a key press as the window reports it: [`chord`], with AltGr and a letter or
/// digit it types nothing for taken as Ctrl+Alt with it, as Windows and Total Commander take it.
/// While AltGr is held on a layout that has it (German among them) winit reports neither Ctrl
/// nor Alt, and the key with its modifiers applied - `modified_key` - as unidentified when AltGr
/// gives it no character: AltGr+F would be a plain F, and the quick search's Ctrl+Alt+F
/// (RC-010-08) out of reach from AltGr. A key AltGr gives a character keeps its chord and its
/// character is typed (`@` from AltGr+Q); a dead key, unidentified too, is no letter.
pub fn pressed(
    key: &Key,
    modified_key: &Key,
    physical: Physical,
    modifiers: Modifiers,
) -> Option<Chord> {
    let mut chord = chord(key, physical, modifiers)?;
    let alphanumeric = matches!(key, Key::Character(text)
        if text.chars().count() == 1 && text.chars().all(char::is_alphanumeric));
    if alphanumeric && matches!(modified_key, Key::Unidentified) && !chord.ctrl && !chord.alt {
        chord.ctrl = true;
        chord.alt = true;
    }
    Some(chord)
}

#[cfg(test)]
#[path = "keymap_tests.rs"]
mod tests;
