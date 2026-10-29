//! What a panel's terminal takes from the app: the shells to offer, their names, and the
//! widget's look from the theme. The sessions are the engine's (`rc-term`); the widget is
//! `rc-term-widget`, the iced_term fork of ADR 0001.

use std::sync::OnceLock;

use iced::keyboard::Modifiers;
use rc_engine::{KeyContext, Keymap};
use rc_term::ShellProfile;
use rc_term_widget::{CopySettings, FontSettings, ReservedKey, Settings};

use crate::{
    i18n::t,
    theme::{self, Tokens},
};

/// The shells the picker offers; filled once.
static SHELLS: OnceLock<Vec<ShellProfile>> = OnceLock::new();

/// This machine's shells, the default first, then one WSL profile per installed distribution -
/// looked up once: the `PATH` scan stats files and WSL is asked through `wsl.exe`, which the UI
/// thread does not do on every Ctrl+Shift+O-umlaut. The start fills it from the engine's
/// `terminal.list` before the window opens ([`set_shells`]); unfilled, the local shells.
pub fn shells() -> &'static [ShellProfile] {
    SHELLS.get_or_init(rc_term::local_shells)
}

/// Keeps `shells` (`terminal.list`'s) as the picker's list; the first call wins.
pub fn set_shells(shells: Vec<ShellProfile>) {
    // A list read before is as good: the second one is dropped.
    let _ = SHELLS.set(shells);
}

/// A shell's name in the picker: the catalogue's for a shell it names, the profile's id for
/// any other (a login shell it does not know, a profile of a later back end).
pub fn shell_label(profile: &ShellProfile) -> String {
    let named = match profile.label_key.as_str() {
        "terminal.shell.pwsh" => t!("terminal.shell.pwsh"),
        "terminal.shell.powershell" => t!("terminal.shell.powershell"),
        "terminal.shell.cmd" => t!("terminal.shell.cmd"),
        "terminal.shell.bash" => t!("terminal.shell.bash"),
        "terminal.shell.zsh" => t!("terminal.shell.zsh"),
        "terminal.shell.fish" => t!("terminal.shell.fish"),
        "terminal.shell.sh" => t!("terminal.shell.sh"),
        "terminal.shell.wsl" => {
            return t!(
                "terminal.shell.wsl",
                distro = profile.distro.as_deref().unwrap_or_default()
            );
        }
        _ => return t!("terminal.shell.other", name = profile.id),
    };
    named.to_owned()
}

/// The chords the app keeps while a terminal has the keyboard: the keymap's `always` bindings
/// and window keys on character keys - the toggle, Ctrl+O-umlaut and Ctrl+`, the terminal over
/// the whole window, Ctrl+A-umlaut and Ctrl+', and the list of the sessions, Ctrl+Shift+A
/// (`term_tests` holds that there are no others).
pub fn reserved_keys(keymap: &Keymap) -> Vec<ReservedKey> {
    let bindings = keymap
        .bindings()
        .iter()
        .map(|binding| (&binding.chord, binding.when));
    let window = keymap.window().iter().map(|key| (&key.chord, key.when));
    bindings
        .chain(window)
        .filter(|(chord, when)| *when == KeyContext::Always && chord.key.chars().count() == 1)
        .map(|(chord, _)| {
            let mut modifiers = Modifiers::empty();
            modifiers.set(Modifiers::CTRL, chord.ctrl);
            modifiers.set(Modifiers::SHIFT, chord.shift);
            modifiers.set(Modifiers::ALT, chord.alt);
            ReservedKey {
                character: chord.key.clone(),
                modifiers,
            }
        })
        .collect()
}

/// The widget's look: the chosen terminal font ([`font`]) and the theme's terminal colours;
/// `reserved` stays with the app, `copy` is how it copies (`terminal.copy_*`).
pub fn settings(tokens: &Tokens, reserved: Vec<ReservedKey>, copy: CopySettings) -> Settings {
    Settings {
        font: font(),
        palette: theme::terminal_palette(tokens),
        reserved,
        copy,
    }
}

/// The terminal's font in force (RC-010-17): Cascadia Mono at 13 until another is chosen.
pub fn font() -> FontSettings {
    let chosen = theme::fonts::fonts().terminal;
    FontSettings {
        size: chosen.size,
        scale_factor: 1.3,
        font_type: chosen.font,
    }
}

#[cfg(test)]
#[path = "term_tests.rs"]
mod tests;
