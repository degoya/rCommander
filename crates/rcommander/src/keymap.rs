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
//! A person reads a key named by place as the active layout prints it ([`label`]). On macOS Cmd is
//! the keymap's Ctrl and keys show as the Mac writes them, while `keys.macos_cmd_as_ctrl` is on
//! (RC-100-49, [`set_cmd_as_ctrl`]); off, Cmd is left to the system as the Windows key is
//! elsewhere, and Ctrl shows as the Mac's Control symbol.

use std::sync::atomic::{AtomicBool, Ordering};

use iced::keyboard::{
    Key, Modifiers,
    key::{Code, Physical},
};
use rc_engine::Chord;

use crate::platform::key_label;

/// `keys.macos_cmd_as_ctrl`, read at the start and after every change (`settings/apply.rs`).
static CMD_AS_CTRL: AtomicBool = AtomicBool::new(true);

/// Whether Cmd is the keymap's Ctrl on macOS, as the setting says.
pub fn set_cmd_as_ctrl(on: bool) {
    CMD_AS_CTRL.store(on, Ordering::Relaxed);
}

/// Whether a press with Cmd counts as Ctrl: on macOS while `keys.macos_cmd_as_ctrl` is on.
fn cmd_as_ctrl() -> bool {
    cfg!(target_os = "macos") && CMD_AS_CTRL.load(Ordering::Relaxed)
}

/// The chord of a key press; `None` for a key no chord can name (a modifier on its own) and for
/// any press with the Windows key, which belongs to the system - or with Cmd, unless it is the
/// keymap's Ctrl ([`cmd_as_ctrl`]).
pub fn chord(key: &Key, physical: Physical, modifiers: Modifiers) -> Option<Chord> {
    chord_with(key, physical, modifiers, cmd_as_ctrl())
}

/// [`chord`], with `cmd_as_ctrl` saying whether the logo key - Cmd - is Ctrl (Cmd+C copies,
/// Cmd+R refreshes; Ctrl stays Ctrl) or the system's.
fn chord_with(
    key: &Key,
    physical: Physical,
    modifiers: Modifiers,
    cmd_as_ctrl: bool,
) -> Option<Chord> {
    if modifiers.logo() && !cmd_as_ctrl {
        return None;
    }
    let ctrl = modifiers.control() || modifiers.logo();
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
        (Key::Unidentified, _) if ctrl || modifiers.alt() => format!("[{}]", code?),
        _ => return None,
    };
    let mut chord = Chord::plain(&name)?;
    chord.ctrl = ctrl;
    chord.shift = modifiers.shift();
    chord.alt = modifiers.alt();
    chord.physical = code;
    Some(chord)
}

/// `chord` as a person reads it: a key named by place as the active keyboard layout prints it,
/// else as a US keyboard does.
pub fn label(chord: &Chord) -> String {
    let text = chord.label(key_label::character);
    if cfg!(target_os = "macos") {
        mac_label(&text, cmd_as_ctrl())
    } else {
        text
    }
}

/// A label as a Mac shows keys: the modifiers as symbols in Apple's order, Control, Option,
/// Shift, Command, with no plus signs. The keymap's Ctrl is Command while `cmd` is on
/// ([`chord`]): `Ctrl+Shift+F` is ⇧⌘F; off, it is Control: ⌃⇧F.
pub fn mac_label(text: &str, cmd: bool) -> String {
    let (mut ctrl, mut shift, mut alt) = (false, false, false);
    let mut rest = text;
    loop {
        if let Some(after) = rest.strip_prefix("Ctrl+") {
            ctrl = true;
            rest = after;
        } else if let Some(after) = rest.strip_prefix("Shift+") {
            shift = true;
            rest = after;
        } else if let Some(after) = rest.strip_prefix("Alt+") {
            alt = true;
            rest = after;
        } else {
            break;
        }
    }
    let key = match rest {
        "" => "+",
        "Plus" => "+",
        "Enter" => "↩",
        "Escape" => "⎋",
        "Backspace" => "⌫",
        "Delete" => "⌦",
        "Tab" => "⇥",
        "Space" => "Space",
        "ArrowUp" | "Up" => "↑",
        "ArrowDown" | "Down" => "↓",
        "ArrowLeft" | "Left" => "←",
        "ArrowRight" | "Right" => "→",
        "PageUp" => "⇞",
        "PageDown" => "⇟",
        "Home" => "↖",
        "End" => "↘",
        other => other,
    };
    let mut label = String::new();
    if ctrl && !cmd {
        label.push('⌃');
    }
    if alt {
        label.push('⌥');
    }
    if shift {
        label.push('⇧');
    }
    if ctrl && cmd {
        label.push('⌘');
    }
    label.push_str(key);
    label
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
