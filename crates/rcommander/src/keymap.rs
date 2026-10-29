//! Key presses spelled as the engine's keymap reads them.
//!
//! The window holds no bindings: [`chord`] turns a key press into an [`rc_engine::Chord`] and
//! [`rc_engine::Keymap::lookup`] names the command. A named key keeps iced's name, the W3C one
//! (`Enter`, `PageDown`); a character key is the character the layout gives without modifiers,
//! in lower case, also with Shift held (iced's key without modifiers): the terminal toggle is
//! Ctrl+O-umlaut on a German layout and Ctrl+` on a US one, both bound, which is one key
//! (`VK_OEM_3`) as in VS Code (owner 2026-10-17). Some keys are named by where they sit rather
//! than by what the layout prints: the key `Backslash` is always `\`, so Ctrl+\ is Ctrl+# on a
//! German keyboard, as Total Commander documents it; and the keypad's grey keys are `NumAdd`,
//! `NumSubtract`, `NumMultiply` and `NumDivide`, apart from `+`, `-`, `*` and `/` elsewhere; the
//! `+` of the main block is `Plus`, as `+` joins a chord's parts (RC-020-12).
//! AltGr with a letter it types nothing for is Ctrl+Alt with it ([`pressed`]).

use iced::keyboard::{
    Key, Modifiers,
    key::{Code, Physical},
};
use rc_engine::Chord;

/// The chord of a key press; `None` for a key no chord can name (a modifier on its own) and for
/// any press with the Windows key, which belongs to the system.
pub fn chord(key: &Key, physical: Physical, modifiers: Modifiers) -> Option<Chord> {
    if modifiers.logo() {
        return None;
    }
    let name = match (key, physical) {
        (_, Physical::Code(Code::Backslash)) => "\\".to_owned(),
        (_, Physical::Code(Code::NumpadAdd)) => "NumAdd".to_owned(),
        (_, Physical::Code(Code::NumpadSubtract)) => "NumSubtract".to_owned(),
        (_, Physical::Code(Code::NumpadMultiply)) => "NumMultiply".to_owned(),
        (_, Physical::Code(Code::NumpadDivide)) => "NumDivide".to_owned(),
        (Key::Named(named), _) => format!("{named:?}"),
        (Key::Character(text), _) if text.as_str() == "+" => "Plus".to_owned(),
        (Key::Character(text), _) => text.to_string(),
        _ => return None,
    };
    let mut chord = Chord::plain(&name)?;
    chord.ctrl = modifiers.control();
    chord.shift = modifiers.shift();
    chord.alt = modifiers.alt();
    Some(chord)
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
