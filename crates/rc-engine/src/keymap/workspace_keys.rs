//! Keys for a workspace (RC-100-68): a key the person adds on the Shortcuts tab ("Add key for a
//! workspace...") that runs `workspaces.apply { id }`. Like a theme key (`theme_keys.rs`) it is no
//! row of the standard keymap: its name is `workspace:<id>`, which the deviations keep like the
//! name of any other row (`Ctrl+Alt+1=workspace:3`), so nothing is stored beside `keys.overrides`.
//!
//! The workspace itself is not checked here - the keymap knows no workspaces: a key whose
//! workspace was deleted stays, and pressing it answers `workspace_unknown` from
//! `workspaces.apply`. The id is read as untrusted input all the same: 1 to
//! [`MAX_WORKSPACE_KEY_DIGITS`] decimal digits, no leading zero.

use super::{Binding, Chord, KeyContext};
use crate::{Command, workspaces::WorkspaceId};

/// What the name of a workspace key starts with.
pub const WORKSPACE_KEY_PREFIX: &str = "workspace:";

/// The most digits a workspace key's id has.
pub const MAX_WORKSPACE_KEY_DIGITS: usize = 18;

/// The name of the key that applies the workspace `id`: `workspace:<id>`.
#[must_use]
pub fn workspace_key_name(id: WorkspaceId) -> String {
    format!("{WORKSPACE_KEY_PREFIX}{id}")
}

/// The workspace id the name of a workspace key holds; `None` for any other name.
#[must_use]
pub fn workspace_key(name: &str) -> Option<WorkspaceId> {
    let digits = name.strip_prefix(WORKSPACE_KEY_PREFIX)?;
    let plain = !digits.is_empty()
        && digits.len() <= MAX_WORKSPACE_KEY_DIGITS
        && !digits.starts_with('0')
        && digits.bytes().all(|byte| byte.is_ascii_digit());
    if !plain {
        return None;
    }
    digits.parse().ok()
}

/// The binding of `chord` to the workspace key `name`; `None` when `name` is none. It applies
/// while the panel shows its files, as most keys do: a terminal keeps its keys.
pub(super) fn workspace_binding(chord: Chord, name: &str) -> Option<Binding> {
    let id = workspace_key(name)?;
    Some(Binding {
        chord,
        when: KeyContext::Files,
        command: Command::ApplyWorkspace { id },
        name: workspace_key_name(id),
    })
}

#[cfg(test)]
#[path = "workspace_keys_tests.rs"]
mod tests;
