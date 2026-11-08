//! The own commands and the Start menu the window knows (RC-100-86): `usercmd.list`'s and
//! `start_menu.list`'s answers, read at the start and again after every `UserCommandsChanged` and
//! `StartMenuChanged`, kept for the run as the workspaces are (`app/workspaces/known.rs`), so the
//! command palette, the menu bar, the Shortcuts tab and the window read the same lists without
//! asking the engine each time they draw.

use std::sync::RwLock;

use rc_engine::{StartItem, UserCommand};

use crate::recover::RwLockOrRecover as _;

static COMMANDS: RwLock<Vec<UserCommand>> = RwLock::new(Vec::new());
static START_MENU: RwLock<Vec<StartItem>> = RwLock::new(Vec::new());

/// The own commands from now on.
pub fn set_commands(list: Vec<UserCommand>) {
    *COMMANDS.write_or_recover("user_commands.commands") = list;
}

/// The Start menu's rows from now on, in menu order.
pub fn set_start_menu(list: Vec<StartItem>) {
    *START_MENU.write_or_recover("user_commands.start_menu") = list;
}

/// Every own command, by name.
pub fn commands() -> Vec<UserCommand> {
    COMMANDS.read_or_recover("user_commands.commands").clone()
}

/// Every row of the Start menu, in menu order: each submenu before its rows.
pub fn start_menu() -> Vec<StartItem> {
    START_MENU
        .read_or_recover("user_commands.start_menu")
        .clone()
}
