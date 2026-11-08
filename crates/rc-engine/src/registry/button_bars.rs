//! The button bars (RC-100-87). The bars reach MCP; which program a button runs is a person's
//! choice, so saving, renaming, deleting and importing them are the window's (like
//! `usercmd.save`); running a button is `Exec`, and `GuiOnly` with files or parameters of the
//! caller's, or when its command starts as administrator, asks for its parameters (`?`) or runs a
//! row stricter than `Exec` (`ESCALATIONS`, `Engine::effective_class`). `view.toggle_bar` shows
//! or hides a bar of the window by its setting; `button_bar.focus` gives a bar the keyboard.

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "button_bar.delete",
        class: CommandClass::GuiOnly,
        summary: "Delete a button bar; not one another bar opens as its sub-bar",
    },
    CommandSpec {
        id: "button_bar.focus",
        class: CommandClass::Read,
        summary: "Give the keyboard to the button bar or the vertical button bar (Alt+Shift+F11, Alt+Shift+F12)",
    },
    CommandSpec {
        id: "button_bar.get",
        class: CommandClass::Read,
        summary: "One button bar with its buttons: commands, sub-bars, symbols, tooltips, separators",
    },
    CommandSpec {
        id: "button_bar.import",
        class: CommandClass::GuiOnly,
        summary: "Import a Total Commander .bar file and its sub-bars as button bars",
    },
    CommandSpec {
        id: "button_bar.import_preview",
        class: CommandClass::GuiOnly,
        summary: "Read a Total Commander .bar file and its sub-bars into what an import would make",
    },
    CommandSpec {
        id: "button_bar.list",
        class: CommandClass::Read,
        summary: "Every button bar with its buttons",
    },
    CommandSpec {
        id: "button_bar.manager",
        class: CommandClass::GuiOnly,
        summary: "Open the button bars' window (Configuration > Button bar...)",
    },
    CommandSpec {
        id: "button_bar.rename",
        class: CommandClass::GuiOnly,
        summary: "Rename a button bar",
    },
    CommandSpec {
        id: "button_bar.run",
        class: CommandClass::Exec,
        summary: "Run a button of a bar for a panel: its program, internal rows or own command; files dropped on a program's button follow its parameters",
    },
    CommandSpec {
        id: "button_bar.save",
        class: CommandClass::GuiOnly,
        summary: "Save a button bar: its name and buttons, each a command or a sub-bar, or a separator",
    },
    CommandSpec {
        id: "view.toggle_bar",
        class: CommandClass::Read,
        summary: "Show or hide a bar of the window: the button bar, the vertical one, the drive bar, the command line, the status bar or the function-key bar",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "button_bar.list" => ListButtonBars {},
            "button_bar.get" => GetButtonBar { id: ButtonBarId },
            /// `id`: over that bar; else a new one.
            "button_bar.save" => SaveButtonBar {
                #[serde(default)]
                id: Option<ButtonBarId>,
                name: String,
                spec: BarSpec,
            },
            "button_bar.rename" => RenameButtonBar { id: ButtonBarId, name: String },
            "button_bar.delete" => DeleteButtonBar { id: ButtonBarId },
            /// The `.bar` file at `path` (absolute) and its sub-bars, read, not imported.
            "button_bar.import_preview" => PreviewButtonBarImport { path: String },
            "button_bar.import" => ImportButtonBar { path: String },
            /// Button `button` (its place among the bar's items, from 0) of bar `bar` for `panel`,
            /// the active one when `None`. `params` replace a program's stored parameters - what
            /// the window sends after asking (`?`); `files` (absolute local paths) follow them.
            "button_bar.run" => RunButton {
                bar: ButtonBarId,
                button: usize,
                #[serde(default)]
                panel: Option<PanelId>,
                #[serde(default)]
                params: Option<String>,
                #[serde(default)]
                files: Vec<String>,
            },
            /// The window opens the button bars' window.
            "button_bar.manager" => ButtonBarManager {},
            /// The window gives the keyboard to the bar at `bar`, when it shows.
            "button_bar.focus" => FocusButtonBar { bar: BarPlace },
            /// The bar's setting turned over.
            "view.toggle_bar" => ToggleBar { bar: BarKind },
        } }
    };
}
pub(super) use commands;
