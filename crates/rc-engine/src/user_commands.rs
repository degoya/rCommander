//! Own commands and the Start menu (RC-100-86), as Total Commander has them: an own command
//! `em_<name>` is a program with parameters, internal rows or another own command
//! ([`CommandRef`], `model.rs`), kept in `rc_db`'s `user_commands`; the Start menu is a tree of
//! entries, submenus and separators in `start_menu_items` (`store.rs`, `start_menu.rs`).
//!
//! Running one (`run.rs`) fills Total Commander's placeholders in a program's parameters
//! (`params.rs`: `%P %N %T %M %O %E %S %R %%`, the list files `%L %F %UL %UF %WL %WF` written
//! into a folder of the user's own that lives until rCommander ends or starts again,
//! `list_files.rs`), and starts the program without a shell, found on the search path and never
//! in the panel's folder (`launch.rs`); on Windows a batch file gets no value cmd would read as
//! syntax. `elevated` starts it as administrator through `commands/elevate.rs` (Windows only).
//! Internal rows are the keymap's names or Total Commander's `cm_*` (`tc_names.rs`), run one
//! after the other; own commands call each other at most [`MAX_DEPTH`] deep.

mod error;
mod judge;
pub(crate) mod launch;
mod list_files;
mod model;
mod params;
mod run;
mod start_menu;
mod store;
pub mod symbols;
pub mod tc_names;

use serde::{Deserialize, Serialize};

pub use error::UserCommandError;
pub(crate) use error::store_error;
pub use launch::{batch_values_are_safe, find_program, is_batch_file, resolve_checked};
pub use list_files::ListKind;
pub use model::{
    CommandKind, CommandRef, MAX_FIELD, MAX_NAME_TAIL, OWN_PREFIX, Show, StartItem, StartItemDraft,
    StartItemId, StartItemKind, UserCommand, UserCommandDraft, UserCommandId, clean_name,
    is_row_name, own_name,
};
pub use params::{
    Expanded, PanelSide, ParamContext, expand_posix, expand_windows, lists_in, marks_only,
};
pub(crate) use run::FileRun;
pub use run::expand_variables;

/// What the tests started instead of starting it, for the associations' tests (RC-100-91).
#[cfg(test)]
pub(crate) use launch::{Launch, STARTED};

use crate::{Engine, EngineEvent, Outcome, PanelId};

/// The most rows of one internal command.
pub const MAX_ROWS: usize = 16;

/// How deep own commands call each other before [`UserCommandError::Recursion`].
pub const MAX_DEPTH: usize = 8;

/// The most own commands, and the most rows of the Start menu.
pub const MAX_USER_COMMANDS: usize = rc_db::MAX_USER_COMMANDS;
pub const MAX_START_MENU_ITEMS: usize = rc_db::MAX_START_MENU_ITEMS;

/// What the `usercmd.*` and `start_menu.*` commands answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum UserCommandsOutcome {
    /// `usercmd.list`: every own command, by name.
    Commands { commands: Vec<UserCommand> },
    /// `start_menu.list`: every row in menu order, each submenu before its rows.
    StartMenu { items: Vec<StartItem> },
    /// `usercmd.save`, `start_menu.save`: the row saved.
    Saved { id: i64 },
    /// `usercmd.delete`, `start_menu.delete`: the rows removed, the row itself counted.
    Deleted { removed: u64 },
    /// `start_menu.move`.
    Done {},
    /// `usercmd.run`, `start_menu.run`: the programs started; `asking` when the window asks for
    /// a program's parameters first (`?`), which then runs it again with them.
    Ran { started: usize, asking: bool },
}

/// Whose parameters the window asks for (`?`): the own command, the Start menu entry or the
/// button that holds the program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum AskSource {
    Own {
        name: String,
    },
    Start {
        id: StartItemId,
    },
    /// A file association's command (RC-100-91): the file, by its path in the panel's
    /// location, and the command's name (`open` for Enter's).
    Association {
        path: String,
        verb: String,
    },
    /// A button of a bar (RC-100-87): its place among the bar's items, and the files dropped on
    /// it, which the run after the question takes again.
    Button {
        bar: crate::ButtonBarId,
        button: usize,
        #[serde(default)]
        files: Vec<String>,
    },
}

/// A program's parameters to show before it starts (`?` in front of them).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskRequest {
    pub source: AskSource,
    /// The panel the command runs for.
    pub panel: PanelId,
    /// The parameters without the `?`, placeholders not yet filled in.
    pub params: String,
    /// What the dialog names: the tooltip, the title or the name.
    pub title: String,
}

/// A `usercmd.*` or `start_menu.*` answer.
pub(crate) fn answer(outcome: UserCommandsOutcome) -> Outcome {
    Outcome::UserCommands(Box::new(outcome))
}

impl Engine {
    /// `usercmd.manager`, `start_menu.manager`: the window opens the own commands' window.
    pub(crate) fn user_commands_window(&self, start_menu: bool) -> Outcome {
        self.publish(EngineEvent::UserCommandsWindow { start_menu });
        Outcome::Panel(self.snapshot(None))
    }
}

/// One command of each variant, for the registry's tests.
#[cfg(test)]
pub(crate) fn one_of_each() -> Vec<crate::Command> {
    use crate::Command;
    let action = CommandRef {
        command: "notepad.exe".into(),
        params: "\"%P%N\"".into(),
        ..CommandRef::default()
    };
    vec![
        Command::ListUserCommands {},
        Command::SaveUserCommand {
            id: None,
            command: UserCommandDraft {
                name: "em_np".into(),
                action: action.clone(),
                tooltip: "Notepad".into(),
                ..UserCommandDraft::default()
            },
        },
        Command::DeleteUserCommand { id: 1 },
        Command::RunUserCommand {
            name: "em_np".into(),
            panel: Some(PanelId::Left),
            params: None,
        },
        Command::UserCommandManager {},
        Command::ListStartMenu {},
        Command::SaveStartItem {
            id: None,
            parent: Some(1),
            item: StartItemDraft {
                kind: StartItemKind::Item,
                title: "Notepad".into(),
                action: Some(action),
            },
        },
        Command::DeleteStartItem { id: 2 },
        Command::MoveStartItem {
            id: 2,
            parent: None,
            position: Some(0),
        },
        Command::RunStartItem {
            id: 2,
            panel: None,
            params: Some("readme.txt".into()),
        },
        Command::StartMenuManager {},
    ]
}

#[cfg(test)]
#[path = "user_commands_tests.rs"]
mod tests;
