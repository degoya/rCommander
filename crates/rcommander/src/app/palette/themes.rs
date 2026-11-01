//! The colour themes in the command palette (RC-100-55): one row per theme and mode, "Theme
//! (dark): Nord", made from the themes the window knows - `theme.list`'s, read at the start and
//! again after every change of them - each time the palette opens, so an own theme appears and
//! goes with the list. A row runs `theme.select { mode, id }`; the rows of the themes the modes
//! draw with are marked. A theme with a key of its own (the Shortcuts tab's "Add key for a
//! theme...") is that key's row, which shows the key. `reach.json` names the command `listed`.
//!
//! The same words name a theme key wherever the name of a key's row is shown - the Shortcuts
//! tab, its question before a key moves, the palette ([`label`]). A key whose theme was deleted
//! names the theme's id and is [`missing`].

use rc_engine::{
    Command,
    settings::themes::{Theme, ThemeMode},
    theme_key, theme_key_name,
};

use super::{Action, Entry};
use crate::{i18n::t, theme};

/// The command whose palette rows a list makes.
#[cfg(test)]
pub const LISTED: &[&str] = &["theme.select"];

/// A theme as the palette offers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeRow {
    pub mode: ThemeMode,
    pub id: String,
    /// Its name as the window shows it, a built-in one's translated.
    pub name: String,
    /// Its mode draws with it.
    pub chosen: bool,
}

/// The themes the window knows, the light ones first, each mode's built-in ones first.
pub fn known() -> Vec<ThemeRow> {
    let mut rows = Vec::new();
    for dark in [false, true] {
        let chosen = theme::themes::chosen_id(dark);
        for theme in theme::themes::of_mode(dark) {
            rows.push(ThemeRow {
                mode: ThemeMode::of(dark),
                name: shown_name(&theme),
                chosen: chosen.as_deref() == Some(theme.id.as_str()),
                id: theme.id,
            });
        }
    }
    rows
}

/// A theme's name as the window shows it: a built-in one's in the window's language.
fn shown_name(theme: &Theme) -> String {
    theme::builtin::name(&theme.id)
        .filter(|_| theme.builtin)
        .map_or_else(|| theme.name.clone(), str::to_owned)
}

/// "Theme (dark): Nord".
pub fn row_label(mode: ThemeMode, name: &str) -> String {
    if mode.is_dark() {
        t!("palette.theme.dark", name = name)
    } else {
        t!("palette.theme.light", name = name)
    }
}

/// The words of the theme key `name` with the themes the window knows; `None` for the name of
/// any other row.
pub fn label(name: &str) -> Option<String> {
    theme_key(name)?;
    label_in(name, &known())
}

/// The words of the theme key `name` among `themes`: the theme's name, or its id when it is
/// none of them; `None` for the name of any other row.
pub fn label_in(name: &str, themes: &[ThemeRow]) -> Option<String> {
    let (mode, id) = theme_key(name)?;
    let shown = themes
        .iter()
        .find(|row| row.mode == mode && row.id == id)
        .map_or(id, |row| row.name.as_str());
    Some(row_label(mode, shown))
}

/// Whether `name` is a theme key whose theme is none of `themes`: deleted since the key was
/// added, or of an import from another machine.
pub fn missing(name: &str, themes: &[ThemeRow]) -> bool {
    theme_key(name)
        .is_some_and(|(mode, id)| !themes.iter().any(|row| row.mode == mode && row.id == id))
}

/// `themes` as rows of the palette among `entries`, all sorted by label again: a theme with a
/// key of its own marks that key's row, every other one is a row of its own that runs
/// `theme.select`.
pub fn add_rows(entries: &mut Vec<Entry>, themes: &[ThemeRow]) {
    for row in themes {
        let name = theme_key_name(row.mode, &row.id);
        if let Some(entry) = entries.iter_mut().find(|entry| entry.name == name) {
            entry.label = row_label(row.mode, &row.name);
            entry.current = row.chosen;
            continue;
        }
        let command = Command::SelectTheme {
            mode: row.mode,
            id: row.id.clone(),
        };
        entries.push(Entry {
            name,
            label: row_label(row.mode, &row.name),
            keys: String::new(),
            chords: String::new(),
            action: Action::Run(Box::new(command)),
            current: row.chosen,
        });
    }
    entries.sort_by_key(|entry| entry.label.to_lowercase());
}

#[cfg(test)]
#[path = "themes_tests.rs"]
mod tests;
