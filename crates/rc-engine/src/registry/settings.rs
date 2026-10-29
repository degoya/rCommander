//! The typed settings: read and change them by key (RC-010-09); the keymap in force and its
//! deviations from the standard, which are a setting too (RC-100-21).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "keymap.list",
        class: CommandClass::Read,
        summary: "The keys in force: every row of the keymap with its key, command, context, whether the person changed it and whether the window keeps it",
    },
    CommandSpec {
        id: "keymap.set",
        class: CommandClass::GuiOnly,
        summary: "Store the deviations from the standard keymap (keys.overrides); only a person moves a key",
    },
    CommandSpec {
        id: "settings.get",
        class: CommandClass::Read,
        summary: "Every setting (or one) with its value, default, type and bounds; never the MCP token",
    },
    CommandSpec {
        id: "settings.set",
        class: CommandClass::Write,
        summary: "Change one setting; MCP and vault settings are refused (only a person changes them)",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "keymap.list" => ListKeymap,
            "keymap.set" => SetKeymap {
                /// The deviations as `keys.overrides` writes them: `Ctrl+K=copy F5=`.
                overrides: String,
            },
            "settings.get" => GetSettings {
                /// Only this setting; every one when `None`.
                #[serde(default)]
                key: Option<String>,
            },
            "settings.set" => SetSetting { key: String, value: Value },
        } }
    };
}
pub(super) use commands;
