//! The keys beside the entries of the menus rCommander draws (RC-010-24): an entry shows the key
//! that runs the same, right-aligned as in a Windows menu ("Close tab   Ctrl+W"); an entry
//! without a key shows none. The tabs' menu takes the key the keymap gives the registry command
//! an entry runs; the file list's own menu, and its entries above the Explorer's, the key they
//! press; the queue's menu the queue window's own keys. The setting
//! `keys.context_menu_shortcuts` (on by default) hides them all; the Explorer's own entries are
//! Windows' and left alone.

use std::sync::atomic::{AtomicBool, Ordering};

use rc_engine::{Chord, Keymap};

use crate::app::palette::menu_bar;

/// `keys.context_menu_shortcuts` off, read with the tabs' settings at the start and after every
/// change (`tabs/reorder.rs`).
static HIDDEN: AtomicBool = AtomicBool::new(false);

/// Whether the menus show their keys.
pub fn set_shown(on: bool) {
    HIDDEN.store(!on, Ordering::Relaxed);
}

/// Whether the menus show their keys now.
pub fn shown() -> bool {
    !HIDDEN.load(Ordering::Relaxed)
}

/// The key the keymap gives the registry command `id` - its row without parameters, as the
/// command palette and the menu bar name it -, as the menu shows it; none without one, or while
/// the keys are hidden.
pub fn of_command(keymap: &Keymap, id: &str) -> Option<String> {
    shown().then(|| menu_bar::key_of(keymap, id)).flatten()
}

/// `chord` as the menu shows it; none while the keys are hidden.
pub fn of_chord(chord: &Chord) -> Option<String> {
    shown().then(|| crate::keymap::label(chord))
}

/// `key`, a window's own key for an entry, while the keys are shown.
pub fn of_text(key: Option<&'static str>) -> Option<&'static str> {
    key.filter(|_| shown())
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
