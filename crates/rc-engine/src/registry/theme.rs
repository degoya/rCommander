//! The colour themes (RC-100-50): the list and the choice of a mode's theme reach MCP - a theme
//! changes what the person sees, never a file; saving and deleting a theme are the window's
//! (`GuiOnly`): the editor with its live preview, the import and the export of a `.rctheme` file
//! are a person's work in the Themes tab.

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "theme.delete",
        class: CommandClass::GuiOnly,
        summary: "Delete an own colour theme; not a built-in one, nor the theme a mode draws with",
    },
    CommandSpec {
        id: "theme.list",
        class: CommandClass::Read,
        summary: "The colour themes: id, name, mode (light or dark), built in or own, and whether it is the mode's theme",
    },
    CommandSpec {
        id: "theme.save",
        class: CommandClass::GuiOnly,
        summary: "Save an own colour theme from the Themes tab's editor or import; never over a built-in one",
    },
    CommandSpec {
        id: "theme.select",
        class: CommandClass::Write,
        summary: "Choose the colour theme of the light or the dark mode; the window draws with it at once",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "theme.list" => ListThemes {},
            "theme.select" => SelectTheme {
                /// The mode whose theme it is.
                mode: ThemeMode,
                /// The theme's id, as `theme.list` names it.
                id: String,
            },
            /// A new theme without an id; the own theme of the id replaced.
            "theme.save" => SaveTheme { theme: ThemeDraft },
            "theme.delete" => DeleteTheme { id: String },
        } }
    };
}
pub(super) use commands;
