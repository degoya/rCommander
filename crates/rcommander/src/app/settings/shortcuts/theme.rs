//! The Shortcuts tab's "Add key for a theme..." (RC-100-55): a panel under the table chooses a
//! theme of either mode - "Theme (dark): Nord", as the command palette names it -, then "Record
//! key" takes the next key pressed as the recording of "Add key" does, the question before a
//! taken key moves included. The key becomes a row of its own, `theme:<mode>:<id>`
//! (`rc_engine::theme_key_name`), which the table shows, changes, removes and resets like any
//! other and OK or Apply store in `keys.overrides`. There is no standard theme key: the person
//! adds the ones wanted.
//!
//! Keys while the panel is open: Enter records, Escape closes it, the arrows and Delete leave the
//! table alone; Tab walks the panel's controls.

use std::fmt;

use rc_engine::{settings::themes::ThemeMode, theme_key_name};

use super::{Recording, Selected};
use crate::app::{App, Effect, palette::themes::ThemeRow};

/// The open panel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThemePick {
    /// The theme key's name of the theme chosen; none before one is.
    pub name: Option<String>,
}

/// A theme in the panel's list: its key's name and its words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeOption {
    pub name: String,
    pub label: String,
}

impl fmt::Display for ThemeOption {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

#[derive(Debug, Clone)]
pub enum ThemeKeyMsg {
    /// "Add key for a theme..." pressed.
    Open,
    Pick(ThemeOption),
    /// "Record key": the next key pressed is the theme's.
    Record,
    Cancel,
}

/// The panel's list: every theme of `themes`, light ones first, as the palette names them.
pub fn options(themes: &[ThemeRow]) -> Vec<ThemeOption> {
    themes
        .iter()
        .map(|row| ThemeOption {
            name: theme_key_name(row.mode, &row.id),
            label: crate::app::palette::themes::row_label(row.mode, &row.name),
        })
        .collect()
}

/// The theme the panel offers first: the one the window's mode draws with now.
fn first(themes: &[ThemeRow], dark: bool) -> Option<String> {
    themes
        .iter()
        .find(|row| row.chosen && row.mode == ThemeMode::of(dark))
        .or_else(|| themes.first())
        .map(|row| theme_key_name(row.mode, &row.id))
}

impl App {
    /// A message of the panel.
    pub(in crate::app::settings) fn theme_key_message(
        &mut self,
        message: ThemeKeyMsg,
    ) -> Vec<Effect> {
        let dark = self.dark;
        let Some(dialog) = self.settings.as_mut() else {
            return Vec::new();
        };
        let page = &mut dialog.shortcuts;
        match message {
            ThemeKeyMsg::Open => {
                if page.recording.is_none() && page.asking.is_none() {
                    let themes = crate::app::palette::themes::known();
                    page.adding = Some(ThemePick {
                        name: first(&themes, dark),
                    });
                    dialog.note = None;
                }
            }
            ThemeKeyMsg::Pick(option) => {
                if let Some(adding) = page.adding.as_mut() {
                    adding.name = Some(option.name);
                }
            }
            ThemeKeyMsg::Record => {
                if let Some(name) = page.adding.take().and_then(|adding| adding.name) {
                    page.selected = Some(Selected {
                        name,
                        keys: String::new(),
                    });
                    page.recording = Some(Recording::Add);
                }
            }
            ThemeKeyMsg::Cancel => page.adding = None,
        }
        Vec::new()
    }
}

#[cfg(test)]
#[path = "theme_tests.rs"]
mod tests;
