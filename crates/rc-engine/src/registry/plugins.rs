//! The installed plugins (RC-100-09, `docs/plugins.md` "MCP"): listing is a read; everything
//! that decides which foreign code runs is the window's alone.

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "plugins.disable",
        class: CommandClass::GuiOnly,
        summary: "Switch an installed plugin off; it is listed but never runs",
    },
    CommandSpec {
        id: "plugins.enable",
        class: CommandClass::GuiOnly,
        summary: "Switch an installed plugin on again",
    },
    CommandSpec {
        id: "plugins.install",
        class: CommandClass::GuiOnly,
        summary: "Install a .rcplug package after its preview, confirming an unknown signing key by its fingerprint",
    },
    CommandSpec {
        id: "plugins.list",
        class: CommandClass::Read,
        summary: "The installed plugins: versions, kind, switch, suspension, permissions and signing key; one plugin with its latest calls",
    },
    CommandSpec {
        id: "plugins.preview",
        class: CommandClass::GuiOnly,
        summary: "What a .rcplug package is before it is installed: name, version, author, claimed files, permissions, key standing",
    },
    CommandSpec {
        id: "plugins.remove",
        class: CommandClass::GuiOnly,
        summary: "Remove one installed version of a plugin; refused while an open viewer uses it",
    },
    CommandSpec {
        id: "plugins.resume",
        class: CommandClass::GuiOnly,
        summary: "Let a plugin suspended after repeated failures run again",
    },
    CommandSpec {
        id: "plugins.rollback",
        class: CommandClass::GuiOnly,
        summary: "Run the previous installed version of a plugin instead of the current one",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "plugins.disable" => DisablePlugin {
                /// The plugin's id (a UUIDv7).
                id: String,
            },
            "plugins.enable" => EnablePlugin {
                id: String,
            },
            /// Installs the package the person previewed; `fingerprint` confirms the signing key
            /// the preview showed, when nobody trusted it yet; `digest` is the preview's SHA-256 of
            /// the file, so a package swapped after the preview is refused (RC-080-53).
            "plugins.install" => InstallPlugin {
                path: String,
                #[serde(default)]
                fingerprint: Option<String>,
                digest: String,
            },
            "plugins.list" => ListPlugins {
                /// Only this plugin, with its latest calls.
                #[serde(default)]
                id: Option<String>,
            },
            "plugins.preview" => PreviewPlugin {
                path: String,
            },
            "plugins.remove" => RemovePlugin {
                id: String,
                version: String,
            },
            "plugins.resume" => ResumePlugin {
                id: String,
            },
            "plugins.rollback" => RollbackPlugin {
                id: String,
            },
        } }
    };
}
pub(super) use commands;
