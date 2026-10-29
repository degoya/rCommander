//! What the drive menu and a drive's context menu offer besides the drives themselves: mapping a
//! network drive and disconnecting one (RC-010-16), as Total Commander's *Net* menu does with
//! Windows' own functions. Neither has a key of its own (owner, 2026-10-18); the drive menu
//! (Alt+F1, Alt+F2) lists them under its drives, a right click on a drive button or a drive
//! menu row opens a drive's context menu with them, and the command palette has both.
//!
//! Pure data, tested without a window.

use rc_engine::{Chord, Drive, DriveGroup, DriveKind, PanelId};

use crate::{
    app::menu_letter::{self, Letter},
    i18n::t,
};

/// An action on network drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveAction {
    /// The dialog that maps a share to a drive letter.
    MapNetwork,
    /// The dialog that disconnects a network drive.
    UnmapNetwork,
}

impl DriveAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::MapNetwork => t!("drives.action_map"),
            Self::UnmapNetwork => t!("drives.action_unmap"),
        }
    }
}

/// Whether `drive` is a mapped network drive (a letter of the Drives group).
pub fn is_network(drive: &Drive) -> bool {
    drive.group == DriveGroup::Drives && drive.kind == DriveKind::Network
}

/// The actions under the drive menu's drives: mapping, and disconnecting while a network drive
/// is listed; none where the platform maps no network drives (`supported`).
pub fn menu_actions(drives: &[Drive], supported: bool) -> Vec<DriveAction> {
    if !supported {
        return Vec::new();
    }
    let mut actions = vec![DriveAction::MapNetwork];
    if drives.iter().any(is_network) {
        actions.push(DriveAction::UnmapNetwork);
    }
    actions
}

/// The actions of `drive`'s context menu: a drive letter offers mapping, a network drive
/// disconnecting too; a WSL distribution or a server offers none.
pub fn context_actions(drive: &Drive, supported: bool) -> Vec<DriveAction> {
    if !supported || drive.group != DriveGroup::Drives {
        return Vec::new();
    }
    let mut actions = vec![DriveAction::MapNetwork];
    if is_network(drive) {
        actions.push(DriveAction::UnmapNetwork);
    }
    actions
}

/// An open context menu of a drive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveContext {
    /// The panel whose bar or menu it was opened from.
    pub panel: PanelId,
    /// The drive's id.
    pub drive: String,
    pub actions: Vec<DriveAction>,
    pub cursor: usize,
}

/// What a key does to the open context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextKey {
    Stay,
    Close,
    Run(DriveAction),
}

impl DriveContext {
    /// Arrows, Home and End move, Enter or Space run the entry, a letter runs the one entry
    /// starting with it (`menu_letter`), Escape and the menu keys close; every other key is
    /// swallowed, as by a Win32 menu.
    pub fn key(&mut self, chord: &Chord) -> ContextKey {
        if menu_letter::is_letter(chord) {
            let labels = self.actions.iter().map(|action| Some(action.label()));
            return match menu_letter::letter(&chord.key, labels, self.cursor) {
                Letter::None => ContextKey::Stay,
                Letter::Move(index) => {
                    self.cursor = index;
                    ContextKey::Stay
                }
                Letter::Take(index) => ContextKey::Run(self.actions[index]),
            };
        }
        let plain = chord.is_plain();
        let last = self.actions.len().saturating_sub(1);
        match chord.key.as_str() {
            "Escape" | "ContextMenu" => ContextKey::Close,
            "F10" if chord.shift => ContextKey::Close,
            "ArrowUp" if plain => {
                self.cursor = self.cursor.saturating_sub(1);
                ContextKey::Stay
            }
            "ArrowDown" if plain => {
                self.cursor = (self.cursor + 1).min(last);
                ContextKey::Stay
            }
            "Home" | "PageUp" if plain => {
                self.cursor = 0;
                ContextKey::Stay
            }
            "End" | "PageDown" if plain => {
                self.cursor = last;
                ContextKey::Stay
            }
            "Enter" | "Space" if plain => self
                .actions
                .get(self.cursor)
                .map_or(ContextKey::Close, |action| ContextKey::Run(*action)),
            _ => ContextKey::Stay,
        }
    }
}

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;
