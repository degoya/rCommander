//! Keys for an own command and a Start menu entry (RC-100-86): like a workspace key
//! (`workspace_keys.rs`) they are no rows of the standard keymap. An own command's key is named
//! by the command itself, `em_<name>` as Total Commander binds it (`Ctrl+Alt+E=em_putty`), a Start
//! menu entry's `start:<id>`; the deviations of `keys.overrides` keep them like any other name.
//!
//! Neither is checked against what is stored - the keymap knows no own commands: a key whose
//! command was deleted stays, and pressing it answers `user_command_unknown` or
//! `start_menu_unknown`. The names are read as untrusted input all the same: `em_` and 1 to 40
//! of `a-z`, `0-9`, `_` (lower case, as Total Commander compares them without regard to case),
//! `start:` and 1 to [`MAX_START_KEY_DIGITS`] decimal digits without a leading zero.

use super::{Binding, Chord, KeyContext};
use crate::{
    Command,
    user_commands::{StartItemId, own_name},
};

/// What the name of a Start menu entry's key starts with.
pub const START_KEY_PREFIX: &str = "start:";

/// The most digits a Start menu key's id has.
pub const MAX_START_KEY_DIGITS: usize = 18;

/// The name of the key that runs the Start menu entry `id`: `start:<id>`.
#[must_use]
pub fn start_key_name(id: StartItemId) -> String {
    format!("{START_KEY_PREFIX}{id}")
}

/// The entry id the name of a Start menu key holds; `None` for any other name.
#[must_use]
pub fn start_key(name: &str) -> Option<StartItemId> {
    let digits = name.strip_prefix(START_KEY_PREFIX)?;
    let plain = !digits.is_empty()
        && digits.len() <= MAX_START_KEY_DIGITS
        && !digits.starts_with('0')
        && digits.bytes().all(|byte| byte.is_ascii_digit());
    if !plain {
        return None;
    }
    digits.parse().ok()
}

/// The own command a key's name runs: the name itself when it is one (`em_putty`); `None` for
/// any other name.
#[must_use]
pub fn own_command_key(name: &str) -> Option<&str> {
    own_name(name).map(|_| name)
}

/// The binding of `chord` to the own command or Start menu key `name`; `None` when `name` is
/// neither. It applies while the panel shows its files, as most keys do: a terminal keeps its
/// keys.
pub(super) fn user_command_binding(chord: Chord, name: &str) -> Option<Binding> {
    let command = if let Some(own) = own_command_key(name) {
        Command::RunUserCommand {
            name: own.to_owned(),
            panel: None,
            params: None,
        }
    } else {
        Command::RunStartItem {
            id: start_key(name)?,
            panel: None,
            params: None,
        }
    };
    Some(Binding {
        chord,
        when: KeyContext::Files,
        command,
        name: name.to_owned(),
    })
}

#[cfg(test)]
#[path = "user_command_keys_tests.rs"]
mod tests;
