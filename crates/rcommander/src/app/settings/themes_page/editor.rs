//! The theme editor of the Themes tab (RC-100-50): the name, the accent (the system's or a colour)
//! and every token in its group, each a swatch, Choose... (the system's colour dialog, as
//! RC-100-43's), its `#rrggbb` and Reset to the value the theme had when the editor opened; the
//! terminal's group with a preview; a warning for every pair below 4.5:1, saving still possible.
//!
//! Every change goes into the edited theme at once, but the look is built only when the typing
//! pauses ([`PAUSE`]): one look per pause, never per key. That look is the live preview - the
//! window, its terminals included, draws the theme's mode with it - and Cancel takes it away, so
//! the theme drawn before is back; Save stores the theme through `theme.save`. The editor ends
//! its preview when it closes in any way, the dialog closing included ([`Drop`]).

use std::{collections::BTreeMap, time::Duration};

use iced::Task;
use rc_engine::settings::themes::{Theme, ThemeAccent, clean_name, hex_colour};

use super::{ThemesMsg, actions, page_message};
use crate::{
    app::{App, Effect},
    i18n::t,
    platform::colour_picker,
    theme::{self, Tokens},
};

/// How long the typing pauses before the look of the edited theme is built.
pub const PAUSE: Duration = Duration::from_millis(250);

/// A colour the editor sets: a token, or the theme's own accent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slot {
    Token(&'static str),
    Accent,
}

#[derive(Debug, Clone)]
pub enum EditorMsg {
    Name(String),
    /// A colour's `#rrggbb` typed; empty for the fixed set's.
    Hex(Slot, String),
    /// The token back to the value the theme had when the editor opened.
    Reset(&'static str),
    /// The system's accent, or the theme's own.
    SystemAccent(bool),
    /// Opens the system's colour dialog on a colour.
    Pick(Slot),
    /// The colour the dialog gave; `None` when it was cancelled.
    Picked(Slot, Option<String>),
    /// The pause after the change of this number: the look is built when no change came since.
    Pause(u64),
    Save,
    Cancel,
}

/// The theme being edited.
#[derive(Debug)]
pub struct Editor {
    /// The number of this editor's previews.
    owner: u64,
    /// The theme when the editor opened: what Reset goes back to.
    origin: Theme,
    /// The theme as edited.
    pub(super) theme: Theme,
    /// What each colour field holds as typed, a half-typed colour included.
    pub(super) typed: BTreeMap<Slot, String>,
    /// The number of the last change.
    changes: u64,
    /// The look of the theme at the last pause: what the swatches and the warnings show.
    pub(super) shown: &'static Tokens,
}

impl Editor {
    /// The editor on `theme`, its look the preview from now on.
    pub fn open(theme: Theme) -> Self {
        let owner = theme::themes::preview_owner();
        theme::themes::set_preview(owner, &theme);
        let mut typed: BTreeMap<Slot, String> = theme
            .tokens
            .iter()
            .filter_map(|(key, value)| {
                let key = theme::themes::KEYS
                    .iter()
                    .copied()
                    .find(|known| *known == key.as_str())?;
                Some((Slot::Token(key), value.clone()))
            })
            .collect();
        if let ThemeAccent::Colour(colour) = &theme.accent {
            typed.insert(Slot::Accent, colour.clone());
        }
        Self {
            owner,
            shown: theme::themes::look(&theme),
            origin: theme.clone(),
            theme,
            typed,
            changes: 0,
        }
    }

    /// The colour `slot` draws now, as the swatch shows it.
    pub(super) fn colour(&self, slot: Slot) -> Option<iced::Color> {
        match slot {
            Slot::Accent => Some(self.shown.accent),
            Slot::Token(key) => theme::themes::seen(self.shown, key),
        }
    }

    /// Whether the token `key` differs from the theme as it opened.
    pub(super) fn changed(&self, key: &str) -> bool {
        self.theme.tokens.get(key) != self.origin.tokens.get(key)
    }

    /// The colour `text` typed into `slot`: taken when it reads, the fixed set's when empty, kept
    /// as typed otherwise.
    fn typed_into(&mut self, slot: Slot, text: String) {
        let colour = hex_colour(&text);
        match slot {
            Slot::Token(key) => match (&colour, text.trim().is_empty()) {
                (Some(colour), _) => {
                    self.theme.tokens.insert(key.to_owned(), colour.clone());
                }
                (None, true) => {
                    self.theme.tokens.remove(key);
                }
                (None, false) => {}
            },
            Slot::Accent => {
                if let Some(colour) = colour {
                    self.theme.accent = ThemeAccent::Colour(colour);
                }
            }
        }
        self.typed.insert(slot, text);
    }

    /// A change: its number, and the pause after which its look is built.
    fn changed_now(&mut self) -> Vec<Effect> {
        self.changes += 1;
        let change = self.changes;
        let pause = Task::perform(tokio::time::sleep(PAUSE), move |()| {
            page_message(ThemesMsg::Editor(EditorMsg::Pause(change)))
        });
        vec![Effect::Task(pause)]
    }

    /// `message` on the editor; what is left to do, and whether the editor closes.
    fn update(&mut self, message: EditorMsg) -> (Vec<Effect>, bool) {
        match message {
            EditorMsg::Name(name) => {
                self.theme.name = name;
                (Vec::new(), false)
            }
            EditorMsg::Hex(slot, text) => {
                self.typed_into(slot, text);
                (self.changed_now(), false)
            }
            EditorMsg::Picked(slot, Some(hex)) => {
                self.typed_into(slot, hex);
                (self.changed_now(), false)
            }
            EditorMsg::Picked(_, None) => (Vec::new(), false),
            EditorMsg::Reset(key) => {
                match self.origin.tokens.get(key) {
                    Some(colour) => {
                        self.theme.tokens.insert(key.to_owned(), colour.clone());
                        self.typed.insert(Slot::Token(key), colour.clone());
                    }
                    None => {
                        self.theme.tokens.remove(key);
                        self.typed.remove(&Slot::Token(key));
                    }
                }
                (self.changed_now(), false)
            }
            EditorMsg::SystemAccent(system) => {
                self.theme.accent = if system {
                    ThemeAccent::System
                } else {
                    let own = theme::themes::hex_of(self.shown.accent);
                    self.typed.insert(Slot::Accent, own.clone());
                    ThemeAccent::Colour(own)
                };
                (self.changed_now(), false)
            }
            EditorMsg::Pick(slot) => {
                let initial = self.colour(slot).map(|colour| {
                    let [red, green, blue, _] = colour.into_rgba8();
                    [red, green, blue]
                });
                let dialog = colour_picker::pick(initial).map(move |chosen| {
                    let hex = chosen.map(|[r, g, b]| format!("#{r:02x}{g:02x}{b:02x}"));
                    page_message(ThemesMsg::Editor(EditorMsg::Picked(slot, hex)))
                });
                (vec![Effect::Task(dialog)], false)
            }
            EditorMsg::Pause(change) if change == self.changes => {
                self.shown = theme::themes::look(&self.theme);
                theme::themes::set_preview(self.owner, &self.theme);
                (Vec::new(), false)
            }
            EditorMsg::Pause(_) | EditorMsg::Save => (Vec::new(), false),
            EditorMsg::Cancel => (Vec::new(), true),
        }
    }

    /// The theme to save, its name as the name rule keeps it.
    pub(super) fn saved(&self) -> Theme {
        Theme {
            name: clean_name(&self.theme.name),
            ..self.theme.clone()
        }
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        theme::themes::end_preview(self.owner);
    }
}

impl App {
    /// A message of the theme editor.
    pub(super) fn theme_editor_message(&mut self, message: EditorMsg) -> Vec<Effect> {
        let engine = self.engine.clone();
        let Some(page) = self.settings.as_mut().map(|dialog| &mut dialog.themes) else {
            return Vec::new();
        };
        let Some(editor) = page.editor.as_mut() else {
            return Vec::new();
        };
        let restyle = matches!(message, EditorMsg::Pause(_) | EditorMsg::Cancel);
        if let EditorMsg::Save = message {
            let theme = editor.saved();
            page.busy = true;
            let command = rc_engine::Command::SaveTheme {
                theme: actions::draft_of(&theme, false),
            };
            let note = t!("settings.themes.saved").to_owned();
            return vec![actions::send(&engine, command, note, None, true)];
        }
        let (effects, close) = editor.update(message);
        if close {
            page.editor = None;
        }
        if restyle {
            self.restyle_terminals();
        }
        effects
    }
}

#[cfg(test)]
#[path = "editor_tests.rs"]
mod tests;
