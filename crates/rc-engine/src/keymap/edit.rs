//! The effective keymap - the standard with the person's deviations ([`Overrides`]) - and the
//! edits of the settings' Shortcuts tab (RC-100-21): a key given to a row of the standard,
//! taken away, a row reset; which keys the window keeps for itself ([`Keymap::locked`]); and
//! the rows the editor and `keymap.list` show ([`KeymapRow`]).
//!
//! Only bindings change. A window key stays where it is: what it does is the window's own state,
//! and the window takes most of them by their chord before the keymap.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::{Binding, Chord, KeyContext, Keymap, Override, Overrides};
use crate::Command;

/// Why the window keeps a chord for itself: no row runs on it but its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lock {
    /// Tab and Shift+Tab walk a dialog's controls.
    Focus,
    /// Escape closes a dialog and stops a wait.
    Escape,
    /// Alt with a letter or digit opens the menu bar's menus and presses a dialog's buttons.
    AccessKey,
    /// A key the window handles itself: the command line's, the clipboard's, the palette's.
    Window,
}

/// Why an edit is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The chord is the window's.
    Locked(Lock),
    /// The standard keymap has no binding of that name.
    UnknownName,
}

/// One row as the Shortcuts tab and `keymap.list` show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeymapRow {
    /// The chord as printed (`Ctrl+K`); empty for a row whose keys were all taken away.
    pub keys: String,
    /// The i18n key of its name is `keys.<name>`.
    pub name: String,
    pub when: KeyContext,
    /// The registry command it runs; none for a key the window handles itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Command>,
    /// It differs from the standard keymap.
    pub changed: bool,
    /// Why it cannot change; none for a row the person may change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<Lock>,
}

impl Keymap {
    /// The standard keymap, read once.
    #[must_use]
    pub fn shared_standard() -> &'static Self {
        static STANDARD: OnceLock<Keymap> = OnceLock::new();
        STANDARD.get_or_init(|| {
            Self::parse(super::DEFAULT).expect("keymap/default.json reads (keymap_tests)")
        })
    }

    /// Why the window keeps `chord` for itself; `None` for a chord the person may bind.
    #[must_use]
    pub fn locked(chord: &Chord) -> Option<Lock> {
        let one_alphanumeric =
            chord.key.chars().count() == 1 && chord.key.chars().all(char::is_alphanumeric);
        if chord.key == "Tab" && !chord.ctrl && !chord.alt {
            Some(Lock::Focus)
        } else if chord.key == "Escape" {
            Some(Lock::Escape)
        } else if chord.alt && !chord.ctrl && one_alphanumeric {
            Some(Lock::AccessKey)
        } else if Self::shared_standard()
            .window()
            .iter()
            .any(|key| key.chord == *chord)
        {
            Some(Lock::Window)
        } else {
            None
        }
    }

    /// The standard keymap with `overrides` applied in their order, and the deviations that no
    /// longer fit - a name the standard does not have, an unbinding of a chord it does not bind.
    #[must_use]
    pub fn with_overrides(overrides: &Overrides) -> (Self, Vec<Override>) {
        let mut keymap = Self::shared_standard().clone();
        let mut stale = Vec::new();
        for row in overrides.rows() {
            let fits = match row {
                Override::Bind { chord, name } => keymap.assign(chord.clone(), name).is_ok(),
                Override::Unbind { chord } => {
                    keymap.bindings.iter().any(|b| b.chord == *chord)
                        && keymap.unassign(chord).is_ok()
                }
            };
            if !fits {
                stale.push(row.clone());
            }
        }
        (keymap, stale)
    }

    /// The deviations of this keymap from the standard.
    #[must_use]
    pub fn overrides(&self) -> Overrides {
        Overrides::between(Self::shared_standard(), self)
    }

    /// The name of the binding or window key on `chord`, whatever it applies to: what an
    /// assignment of `chord` would replace.
    #[must_use]
    pub fn holder(&self, chord: &Chord) -> Option<&str> {
        self.bindings
            .iter()
            .find(|binding| binding.chord == *chord)
            .map(|binding| binding.name.as_str())
            .or_else(|| {
                self.window
                    .iter()
                    .find(|key| key.chord == *chord)
                    .map(|key| key.name.as_str())
            })
    }

    /// `chord` runs the standard binding `name` from now on, beside its other keys; whatever
    /// ran on it before no longer does, and a reservation of it ends.
    ///
    /// # Errors
    ///
    /// The chord is the window's, or the standard has no binding `name`.
    pub fn assign(&mut self, chord: Chord, name: &str) -> Result<(), Refusal> {
        if let Some(lock) = Self::locked(&chord) {
            return Err(Refusal::Locked(lock));
        }
        let template = Self::shared_standard()
            .bindings
            .iter()
            .find(|binding| binding.name == name)
            .ok_or(Refusal::UnknownName)?;
        self.place(Binding {
            chord,
            ..template.clone()
        });
        Ok(())
    }

    /// `chord` runs nothing any more.
    ///
    /// # Errors
    ///
    /// The chord is the window's.
    pub fn unassign(&mut self, chord: &Chord) -> Result<(), Refusal> {
        if let Some(lock) = Self::locked(chord) {
            return Err(Refusal::Locked(lock));
        }
        self.bindings.retain(|binding| binding.chord != *chord);
        Ok(())
    }

    /// `name` with the keys the standard gives it, taken back from the rows that hold them now.
    pub fn reset(&mut self, name: &str) {
        self.bindings.retain(|binding| binding.name != name);
        let standard = Self::shared_standard();
        for binding in standard.bindings.iter().filter(|b| b.name == name) {
            self.place(binding.clone());
        }
    }

    /// `binding` after the last row of its name - or at the end -, alone on its chord.
    fn place(&mut self, binding: Binding) {
        self.bindings.retain(|kept| kept.chord != binding.chord);
        self.reserved.retain(|kept| kept.chord != binding.chord);
        let at = self
            .bindings
            .iter()
            .rposition(|kept| kept.name == binding.name)
            .map_or(self.bindings.len(), |at| at + 1);
        self.bindings.insert(at, binding);
    }

    /// The first key of the binding `name`; none when it has no key.
    #[must_use]
    pub fn key_of(&self, name: &str) -> Option<&Chord> {
        self.bindings
            .iter()
            .find(|binding| binding.name == name)
            .map(|binding| &binding.chord)
    }

    /// The key that runs here what `chord` runs in the standard keymap: `chord` itself unless the
    /// person moved that binding; none when it has no key left (or the standard binds nothing on
    /// `chord`).
    #[must_use]
    pub fn moved_key(&self, chord: &Chord) -> Option<&Chord> {
        let name = Self::shared_standard()
            .bindings
            .iter()
            .find(|binding| binding.chord == *chord)?
            .name
            .as_str();
        let unmoved = self
            .bindings
            .iter()
            .find(|binding| binding.chord == *chord && binding.name == name);
        unmoved
            .map(|binding| &binding.chord)
            .or_else(|| self.key_of(name))
    }

    /// The standard bindings whose name has no key here, the first of each name: the command
    /// palette still offers them.
    #[must_use]
    pub fn without_key(&self) -> Vec<&'static Binding> {
        let mut found: Vec<&'static Binding> = Vec::new();
        for binding in &Self::shared_standard().bindings {
            let bound = self.bindings.iter().any(|b| b.name == binding.name);
            if !bound && !found.iter().any(|b| b.name == binding.name) {
                found.push(binding);
            }
        }
        found
    }

    /// Every row, by name in the standard's order: each binding and window key of the name, or
    /// one row without keys when they were all taken away.
    #[must_use]
    pub fn rows(&self) -> Vec<KeymapRow> {
        let standard = Self::shared_standard();
        let mut names: Vec<&str> = Vec::new();
        let all = standard
            .bindings
            .iter()
            .chain(&self.bindings)
            .map(|binding| binding.name.as_str())
            .chain(standard.window.iter().map(|key| key.name.as_str()));
        for name in all {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        let mut rows = Vec::new();
        for name in names {
            let before = rows.len();
            for binding in self.bindings.iter().filter(|b| b.name == name) {
                let kept = standard
                    .bindings
                    .iter()
                    .any(|b| b.chord == binding.chord && b.name == name);
                rows.push(KeymapRow {
                    keys: binding.chord.to_string(),
                    name: name.to_owned(),
                    when: binding.when,
                    command: Some(binding.command.clone()),
                    changed: !kept,
                    locked: Self::locked(&binding.chord),
                });
            }
            for key in self.window.iter().filter(|key| key.name == name) {
                rows.push(KeymapRow {
                    keys: key.chord.to_string(),
                    name: name.to_owned(),
                    when: key.when,
                    command: None,
                    changed: !standard.window.contains(key),
                    locked: Some(Lock::Window),
                });
            }
            let template = standard.bindings.iter().find(|b| b.name == name);
            if let (true, Some(template)) = (rows.len() == before, template) {
                rows.push(KeymapRow {
                    keys: String::new(),
                    name: name.to_owned(),
                    when: template.when,
                    command: Some(template.command.clone()),
                    changed: true,
                    locked: None,
                });
            }
        }
        rows
    }
}

#[cfg(test)]
#[path = "edit_tests.rs"]
mod tests;
