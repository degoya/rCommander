//! Keys for a colour theme (RC-100-55): a key the person adds on the Shortcuts tab ("Add key for
//! a theme...") that runs `theme.select { mode, id }`. It is no row of the standard keymap: its
//! name is `theme:<mode>:<id>`, which the deviations keep like the name of any other row
//! (`Ctrl+1=theme:dark:nord-dark`), so it goes with an export and an import of the settings
//! (RC-080-19) and needs nothing stored beside `keys.overrides`.
//!
//! The theme itself is not checked here - the keymap knows no themes: a key whose theme was
//! deleted stays, and pressing it answers `theme_unknown` from `theme.select`, as the Shortcuts
//! tab marks it. The id is read as untrusted input all the same: 1 to [`MAX_THEME_KEY_ID`] of
//! `a-z`, `0-9` and `-`, the characters of every theme id.

use super::{Binding, Chord, KeyContext};
use crate::{Command, settings::themes::ThemeMode};

/// What the name of a theme key starts with.
pub const THEME_KEY_PREFIX: &str = "theme:";

/// The longest theme id a theme key names.
pub const MAX_THEME_KEY_ID: usize = 64;

/// The name of the key that chooses the theme `id` for `mode`: `theme:<mode>:<id>`.
#[must_use]
pub fn theme_key_name(mode: ThemeMode, id: &str) -> String {
    let mode = if mode.is_dark() { "dark" } else { "light" };
    format!("{THEME_KEY_PREFIX}{mode}:{id}")
}

/// The mode and the theme id the name of a theme key holds; `None` for any other name.
#[must_use]
pub fn theme_key(name: &str) -> Option<(ThemeMode, &str)> {
    let (mode, id) = name.strip_prefix(THEME_KEY_PREFIX)?.split_once(':')?;
    let mode = match mode {
        "light" => ThemeMode::Light,
        "dark" => ThemeMode::Dark,
        _ => return None,
    };
    let plain = !id.is_empty()
        && id.len() <= MAX_THEME_KEY_ID
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    plain.then_some((mode, id))
}

/// The binding of `chord` to the theme key `name`; `None` when `name` is none. It applies while
/// the panel shows its files, as most keys do: a terminal keeps its keys.
pub(super) fn theme_binding(chord: Chord, name: &str) -> Option<Binding> {
    let (mode, id) = theme_key(name)?;
    Some(Binding {
        chord,
        when: KeyContext::Files,
        command: Command::SelectTheme {
            mode,
            id: id.to_owned(),
        },
        name: theme_key_name(mode, id),
    })
}

#[cfg(test)]
#[path = "theme_keys_tests.rs"]
mod tests;
