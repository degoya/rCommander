//! The colour themes of the active workspace (RC-100-68): while a workspace with a theme of its
//! own for a mode is active, the window draws that mode with it, ahead of the theme the settings
//! choose (`appearance.theme_light`, `.theme_dark`), which stays as it is. A workspace without a
//! theme for the mode, a theme the window does not know (deleted) or of the other mode, and no
//! active workspace all draw with the chosen theme again. The editor's preview comes first: what
//! is being edited is what is seen ([`current`](super::current)).
//!
//! The themes are set with every list of the workspaces (`app/workspaces/known.rs`); their looks
//! are found then and again whenever the themes are read ([`resolve`]), so drawing only reads
//! them.

use std::sync::RwLock;

use crate::recover::RwLockOrRecover as _;
use crate::theme::Tokens;

/// The active workspace's theme of the light and of the dark mode, by id.
static ACTIVE: RwLock<[Option<String>; 2]> = RwLock::new([None, None]);
/// Their looks, light first; `None` for a mode without one the window knows.
static LOOKS: RwLock<[Option<&'static Tokens>; 2]> = RwLock::new([None, None]);

/// The active workspace's themes from now on, light first; whether they changed.
pub(super) fn set_themes(themes: [Option<String>; 2]) -> bool {
    let mut active = ACTIVE.write_or_recover("workspace.active");
    if *active == themes {
        return false;
    }
    *active = themes;
    true
}

/// The looks of the active workspace's themes found again with `look` (by id and mode).
pub(super) fn resolve(look: impl Fn(&str, bool) -> Option<&'static Tokens>) {
    let active = ACTIVE.read_or_recover("workspace.active").clone();
    let looks = [false, true].map(|dark| {
        active[usize::from(dark)]
            .as_deref()
            .and_then(|id| look(id, dark))
    });
    *LOOKS.write_or_recover("workspace.looks") = looks;
}

/// The look of the active workspace's theme of the light or the `dark` mode, if it has one the
/// window knows.
pub(super) fn look(dark: bool) -> Option<&'static Tokens> {
    LOOKS.read_or_recover("workspace.looks")[usize::from(dark)]
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
