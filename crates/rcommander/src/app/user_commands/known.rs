//! The own commands and the Start menu the window knows (RC-100-86): `usercmd.list`'s and
//! `start_menu.list`'s answers, read at the start and again after every `UserCommandsChanged` and
//! `StartMenuChanged`, kept for the run as the workspaces are (`app/workspaces/known.rs`), so the
//! command palette, the menu bar, the Shortcuts tab and the window read the same lists without
//! asking the engine each time they draw.

use rc_engine::{StartItem, UserCommand};

use crate::app::known::Known;

static COMMANDS: Known<Vec<UserCommand>> = Known::new("user_commands.commands", Vec::new());
static START_MENU: Known<Vec<StartItem>> = Known::new("user_commands.start_menu", Vec::new());

/// The own commands from now on.
pub fn set_commands(list: Vec<UserCommand>) {
    COMMANDS.set(list);
}

/// The Start menu's rows from now on, in menu order.
pub fn set_start_menu(list: Vec<StartItem>) {
    START_MENU.set(list);
}

/// Every own command, by name.
pub fn commands() -> Vec<UserCommand> {
    COMMANDS.get()
}

/// `look` at the own commands without a copy of them (RC-110-29): for what reads them at every
/// draw, as the button bar does per button. `look` runs under the list's lock, so it reads and
/// returns - it neither sets the list nor asks for it again.
pub fn with_commands<T>(look: impl FnOnce(&[UserCommand]) -> T) -> T {
    COMMANDS.with(|list| look(list.as_slice()))
}

/// Every row of the Start menu, in menu order: each submenu before its rows.
pub fn start_menu() -> Vec<StartItem> {
    START_MENU.get()
}

#[cfg(test)]
#[path = "known_tests.rs"]
mod tests;
