//! The Keyboard page's Shortcuts tab (RC-100-21), after Total Commander's Options > Misc >
//! Hotkeys: every key rCommander runs in one table - its action, its key, where it applies -
//! with a search by name or key; for the selected row "Change key" and "Add key" take the next
//! key pressed, "Remove key" takes it away, "Reset" gives the row its standard keys again and
//! "Reset all" every row. A key another row holds asks first whether to take it from that row; a
//! key the window keeps (Tab, Escape, Alt with a letter, the window's own keys) is refused with
//! its reason, and the rows that hold one say why in a tooltip. Changed rows are marked.
//!
//! What the tab edits is the draft's text of the setting `keys.overrides`, through the engine's
//! keymap ([`Keymap::with_overrides`], [`Keymap::overrides`]); OK and Apply store it with
//! `keymap.set` like every other change, and the window takes the new keys at once
//! (`apply.rs`).
//!
//! Keys while the tab is shown: the arrows select a row, Delete removes its key; while a key is
//! being recorded every key is the recording's - Escape cancels it - and nothing else of the
//! window sees it ([`App::shortcut_recording`]); while the question is open Enter takes the key
//! and Escape keeps it where it is.

use rc_engine::{Chord, Keymap, KeymapRow, Lock, Overrides, Refusal, settings::KEYMAP_KEY};

use super::{
    Page, SettingsMessage, Tab,
    draft::{Draft, Field},
    wrap,
};
use crate::{
    app::{App, Effect, Message},
    i18n::{self, t},
};

/// What a recorded key does to the selected row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recording {
    /// It replaces the row's key (or gives a row without a key its first).
    Change,
    /// It is one more key of the row's action.
    Add,
}

/// A row of the table, by its action's name and its key (empty for a row without one).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    pub name: String,
    pub keys: String,
}

/// A recorded key another row holds: the question whether to take it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub chord: Chord,
    /// The name of the row that holds it.
    pub holder: String,
    recording: Recording,
}

/// What the tab keeps while the dialog is open.
#[derive(Debug, Default)]
pub struct ShortcutsPage {
    /// The search, as typed.
    pub search: String,
    pub selected: Option<Selected>,
    /// The next key pressed is the selected row's.
    pub recording: Option<Recording>,
    pub asking: Option<Conflict>,
}

#[derive(Debug, Clone)]
pub enum ShortcutsMsg {
    Search(String),
    Select(Selected),
    Record(Recording),
    Remove,
    Reset,
    ResetAll,
    /// The question's yes: the key moves to the selected row.
    Replace,
    /// The question's no, or Escape while recording.
    KeepIt,
}

pub fn page_message(message: ShortcutsMsg) -> Message {
    wrap(SettingsMessage::Shortcuts(message))
}

/// The keymap the draft's deviations make.
pub fn keymap_of(draft: &Draft) -> Keymap {
    let text = match draft.field(KEYMAP_KEY) {
        Some(Field::Text(text)) => text.as_str(),
        _ => "",
    };
    Keymap::with_overrides(&Overrides::parse(text).unwrap_or_default()).0
}

/// `keymap`'s deviations into the draft.
fn store(draft: &mut Draft, keymap: &Keymap) {
    draft.edit(KEYMAP_KEY, Field::Text(keymap.overrides().to_string()));
}

/// The rows of `keymap` whose action or key holds every word of `search`, ignoring case.
pub fn shown(keymap: &Keymap, search: &str) -> Vec<KeymapRow> {
    let search = search.to_lowercase();
    let words: Vec<&str> = search.split_whitespace().collect();
    keymap
        .rows()
        .into_iter()
        .filter(|row| {
            let text = format!("{} {}", label(&row.name), row.keys).to_lowercase();
            words.iter().all(|word| text.contains(word))
        })
        .collect()
}

/// The action's name as the palette shows it.
pub fn label(name: &str) -> String {
    i18n::code_text("keys", name)
}

/// Why the window keeps a key.
pub fn lock_reason(lock: Lock) -> &'static str {
    match lock {
        Lock::Focus => t!("settings.shortcuts.locked.focus"),
        Lock::Escape => t!("settings.shortcuts.locked.escape"),
        Lock::AccessKey => t!("settings.shortcuts.locked.access_key"),
        Lock::Window => t!("settings.shortcuts.locked.window"),
    }
}

/// The row of `keymap` that is `selected`.
pub fn row_of<'r>(rows: &'r [KeymapRow], selected: &Selected) -> Option<&'r KeymapRow> {
    rows.iter()
        .find(|row| row.name == selected.name && row.keys == selected.keys)
}

/// The selected row's action with its first key now, or none.
fn reselect(keymap: &Keymap, name: &str) -> Selected {
    Selected {
        name: name.to_owned(),
        keys: keymap
            .key_of(name)
            .map(ToString::to_string)
            .unwrap_or_default(),
    }
}

impl App {
    /// Whether the Shortcuts tab is the one shown.
    fn shortcuts_shown(&self) -> bool {
        self.settings
            .as_ref()
            .is_some_and(|dialog| dialog.page == Page::Keyboard)
            && self.settings_tabs.chosen(Page::Keyboard) == Some(Tab::Shortcuts)
    }

    /// A message of the tab.
    pub(super) fn shortcuts_message(&mut self, message: ShortcutsMsg) -> Vec<Effect> {
        let Some(dialog) = self.settings.as_mut() else {
            return Vec::new();
        };
        let page = &mut dialog.shortcuts;
        let Some(draft) = dialog.draft.as_mut() else {
            return Vec::new();
        };
        let mut keymap = keymap_of(draft);
        match message {
            ShortcutsMsg::Search(search) => page.search = search,
            ShortcutsMsg::Select(selected) => {
                page.selected = Some(selected);
                page.recording = None;
                page.asking = None;
            }
            ShortcutsMsg::Record(recording) => {
                if page.selected.is_some() && page.asking.is_none() {
                    page.recording = Some(recording);
                    dialog.note = None;
                }
            }
            ShortcutsMsg::Remove => {
                let Some(selected) = page.selected.clone() else {
                    return Vec::new();
                };
                if let Ok(chord) = selected.keys.parse::<Chord>() {
                    match keymap.unassign(&chord) {
                        Ok(()) => {
                            store(draft, &keymap);
                            page.selected = Some(reselect(&keymap, &selected.name));
                        }
                        Err(refusal) => dialog.note = Some((refused(&chord, refusal), true)),
                    }
                }
            }
            ShortcutsMsg::Reset => {
                if let Some(selected) = page.selected.clone() {
                    keymap.reset(&selected.name);
                    store(draft, &keymap);
                    page.selected = Some(reselect(&keymap, &selected.name));
                }
            }
            ShortcutsMsg::ResetAll => {
                draft.edit(KEYMAP_KEY, Field::Text(String::new()));
                page.selected = None;
                page.asking = None;
            }
            ShortcutsMsg::Replace => {
                let asked = page.asking.take();
                if let (Some(conflict), Some(selected)) = (asked, page.selected.clone()) {
                    let done = commit(&mut keymap, &selected, conflict.recording, conflict.chord);
                    match done {
                        Ok(now) => {
                            store(draft, &keymap);
                            page.selected = Some(now);
                        }
                        Err(note) => dialog.note = Some((note, true)),
                    }
                }
            }
            ShortcutsMsg::KeepIt => {
                page.asking = None;
                page.recording = None;
            }
        }
        Vec::new()
    }

    /// Whether the Shortcuts tab records a key just now.
    pub(in crate::app) fn records_shortcut(&self) -> bool {
        self.settings
            .as_ref()
            .is_some_and(|settings| settings.shortcuts.recording.is_some())
    }

    /// While a key is recorded, every key press is the recording's - before F10, Escape and
    /// every dialog see it; `None` while nothing is recorded.
    pub(in crate::app) fn shortcut_recording(&mut self, message: &Message) -> Option<Vec<Effect>> {
        let recording = self.settings.as_ref()?.shortcuts.recording?;
        let chord = match message {
            Message::Key(chord) => chord.clone(),
            // A key a control took - the Enter that pressed "Change key" - is no new key.
            Message::KeyCaptured(_) => return Some(Vec::new()),
            _ => return None,
        };
        let plain = chord.is_plain();
        if chord.key == "Escape" && plain {
            return Some(self.shortcuts_message(ShortcutsMsg::KeepIt));
        }
        let dialog = self.settings.as_mut()?;
        let draft = dialog.draft.as_mut()?;
        let page = &mut dialog.shortcuts;
        let selected = page.selected.clone()?;
        if let Some(lock) = Keymap::locked(&chord) {
            // The recording goes on: another key may follow.
            dialog.note = Some((refused(&chord, Refusal::Locked(lock)), true));
            return Some(Vec::new());
        }
        page.recording = None;
        dialog.note = None;
        let mut keymap = keymap_of(draft);
        match keymap.holder(&chord).map(str::to_owned) {
            Some(holder) if holder != selected.name => {
                page.asking = Some(Conflict {
                    holder,
                    chord,
                    recording,
                });
            }
            _ => match commit(&mut keymap, &selected, recording, chord) {
                Ok(now) => {
                    store(draft, &keymap);
                    page.selected = Some(now);
                }
                Err(note) => dialog.note = Some((note, true)),
            },
        }
        Some(Vec::new())
    }

    /// The tab's keys while it is shown and nothing is recorded; `None` for a key the dialog
    /// handles itself.
    pub(super) fn shortcuts_key(&mut self, chord: &Chord) -> Option<Vec<Effect>> {
        if !self.shortcuts_shown() || !chord.is_plain() {
            return None;
        }
        let dialog = self.settings.as_mut()?;
        let key = chord.key.as_str();
        if dialog.shortcuts.asking.is_some() {
            return match key {
                "Enter" => Some(self.shortcuts_message(ShortcutsMsg::Replace)),
                "Escape" => Some(self.shortcuts_message(ShortcutsMsg::KeepIt)),
                _ => Some(Vec::new()),
            };
        }
        let step: isize = match key {
            "ArrowUp" => -1,
            "ArrowDown" => 1,
            "Delete" => return Some(self.shortcuts_message(ShortcutsMsg::Remove)),
            _ => return None,
        };
        let draft = dialog.draft.as_ref()?;
        let page = &mut dialog.shortcuts;
        let rows = shown(&keymap_of(draft), &page.search);
        let at = page
            .selected
            .as_ref()
            .and_then(|selected| row_of(&rows, selected))
            .and_then(|row| rows.iter().position(|r| r == row));
        let next = match at {
            None => 0,
            Some(at) => at
                .saturating_add_signed(step)
                .min(rows.len().saturating_sub(1)),
        };
        if let Some(row) = rows.get(next) {
            page.selected = Some(Selected {
                name: row.name.clone(),
                keys: row.keys.clone(),
            });
        }
        Some(Vec::new())
    }
}

/// `chord` given to the `selected` row as `recording` says: the row as it is after.
fn commit(
    keymap: &mut Keymap,
    selected: &Selected,
    recording: Recording,
    chord: Chord,
) -> Result<Selected, String> {
    if recording == Recording::Change
        && let Ok(old) = selected.keys.parse::<Chord>()
        && old != chord
    {
        keymap
            .unassign(&old)
            .map_err(|refusal| refused(&old, refusal))?;
    }
    let keys = chord.to_string();
    keymap
        .assign(chord.clone(), &selected.name)
        .map_err(|refusal| refused(&chord, refusal))?;
    Ok(Selected {
        name: selected.name.clone(),
        keys,
    })
}

/// The note of a refused edit.
fn refused(chord: &Chord, refusal: Refusal) -> String {
    let reason = match refusal {
        Refusal::Locked(lock) => lock_reason(lock),
        Refusal::UnknownName => t!("settings.shortcuts.unknown"),
    };
    t!("settings.shortcuts.refused", key = chord, reason = reason)
}

#[cfg(test)]
#[path = "shortcuts_tests.rs"]
mod tests;
