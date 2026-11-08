//! Own commands `em_<name>` and the Start menu (RC-100-86). The lists reach MCP; which program
//! runs is a person's choice, so saving, deleting and moving are the window's (like
//! `files.viewer`); running one is `Exec`, and `GuiOnly` when it starts as administrator, asks
//! for its parameters (`?`) or gets them from the caller (`ESCALATIONS`, `Engine::effective_class`).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "start_menu.delete",
        class: CommandClass::GuiOnly,
        summary: "Delete a row of the Start menu; a submenu with everything in it",
    },
    CommandSpec {
        id: "start_menu.list",
        class: CommandClass::Read,
        summary: "The Start menu in menu order: entries with their commands, submenus, separators",
    },
    CommandSpec {
        id: "start_menu.manager",
        class: CommandClass::GuiOnly,
        summary: "Open the own commands' window at its Start menu tab (Change Start menu...)",
    },
    CommandSpec {
        id: "start_menu.move",
        class: CommandClass::GuiOnly,
        summary: "Give a row of the Start menu another place or submenu",
    },
    CommandSpec {
        id: "start_menu.run",
        class: CommandClass::Exec,
        summary: "Run an entry of the Start menu for a panel: its program, internal rows or own command",
    },
    CommandSpec {
        id: "start_menu.save",
        class: CommandClass::GuiOnly,
        summary: "Save a row of the Start menu: an entry with its command, a submenu or a separator",
    },
    CommandSpec {
        id: "usercmd.delete",
        class: CommandClass::GuiOnly,
        summary: "Delete an own command",
    },
    CommandSpec {
        id: "usercmd.list",
        class: CommandClass::Read,
        summary: "The own commands (em_) with their programs, parameters, rows and tooltips",
    },
    CommandSpec {
        id: "usercmd.manager",
        class: CommandClass::GuiOnly,
        summary: "Open the own commands' window",
    },
    CommandSpec {
        id: "usercmd.run",
        class: CommandClass::Exec,
        summary: "Run an own command (em_) for a panel: its program with the placeholders filled in, its internal rows or another own command",
    },
    CommandSpec {
        id: "usercmd.save",
        class: CommandClass::GuiOnly,
        summary: "Save an own command: a program with parameters, internal rows or another own command",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "usercmd.list" => ListUserCommands {},
            /// `id`: over that command; else a new one.
            "usercmd.save" => SaveUserCommand {
                #[serde(default)]
                id: Option<UserCommandId>,
                command: UserCommandDraft,
            },
            "usercmd.delete" => DeleteUserCommand { id: UserCommandId },
            /// The command `name` (`em_...`, case does not count) for `panel`, the active one when
            /// `None`. `params` replace a program's stored parameters - what the window sends
            /// after asking (`?`).
            "usercmd.run" => RunUserCommand {
                name: String,
                #[serde(default)]
                panel: Option<PanelId>,
                #[serde(default)]
                params: Option<String>,
            },
            /// The window opens the own commands' window.
            "usercmd.manager" => UserCommandManager {},
            "start_menu.list" => ListStartMenu {},
            /// `id`: over that row, which keeps its place (`parent` ignored); else a new row after
            /// the last one of `parent` (the top when `None`).
            "start_menu.save" => SaveStartItem {
                #[serde(default)]
                id: Option<StartItemId>,
                #[serde(default)]
                parent: Option<StartItemId>,
                item: StartItemDraft,
            },
            /// A submenu goes with everything in it.
            "start_menu.delete" => DeleteStartItem { id: StartItemId },
            /// Into `parent` (the top when `None`) at `position` among its rows, from 0; after the
            /// last when `None` or beyond it.
            "start_menu.move" => MoveStartItem {
                id: StartItemId,
                #[serde(default)]
                parent: Option<StartItemId>,
                #[serde(default)]
                position: Option<usize>,
            },
            /// The entry `id` for `panel`; `params` as for `usercmd.run`.
            "start_menu.run" => RunStartItem {
                id: StartItemId,
                #[serde(default)]
                panel: Option<PanelId>,
                #[serde(default)]
                params: Option<String>,
            },
            /// The window opens the own commands' window at its Start menu tab.
            "start_menu.manager" => StartMenuManager {},
        } }
    };
}
pub(super) use commands;
