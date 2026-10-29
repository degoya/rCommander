//! Packing and unpacking archives as jobs of the operations queue (RC-030-03, RC-030-04).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "archive.extract",
        class: CommandClass::Write,
        summary: "Unpack archives into a directory as a queued job: each into its own folder, by mask, with or without paths",
    },
    CommandSpec {
        id: "archive.pack",
        class: CommandClass::Write,
        summary: "Pack files and folders into a new zip, 7z or tar archive (gz, xz, bz2, zst) as a queued job",
    },
    CommandSpec {
        id: "archive.password",
        class: CommandClass::GuiOnly,
        summary: "Answer an extraction's password question; the password lives for that job only",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            /// Alt+F9 (RC-030-03). From a key without `target` the window first opens its dialog.
            "archive.extract" => ArchiveExtract {
                #[serde(default)]
                panel: Option<PanelId>,
                /// Absolute, or relative to the panel's directory. Empty: the selection, else the
                /// cursor.
                #[serde(default)]
                archives: Vec<String>,
                /// The directory to unpack into; the other panel's when `None`.
                #[serde(default)]
                target: Option<String>,
                #[serde(default)]
                settings: rc_ops::ExtractSettings,
                #[serde(default)]
                options: JobOptions,
            },
            /// Alt+F5, Alt+Shift+F5 with `delete_sources` (RC-030-04). From a key without `target`
            /// the window first opens its dialog.
            "archive.pack" => ArchivePack {
                #[serde(default)]
                panel: Option<PanelId>,
                /// Absolute, or relative to the panel's directory. Empty: the selection, else the
                /// cursor.
                #[serde(default)]
                sources: Vec<String>,
                /// The archive (the directory with `each_own_archive`); `None` names it after the
                /// source in the other panel's directory.
                #[serde(default)]
                target: Option<String>,
                #[serde(default)]
                settings: rc_ops::PackSettings,
                #[serde(default)]
                options: JobOptions,
            },
            /// The password of an encrypted archive an extraction asks for; `None` skips its
            /// encrypted entries.
            "archive.password" => ArchivePassword {
                job: JobId,
                #[serde(default)]
                password: Option<SecretParam>,
            },
        } }
    };
}
pub(super) use commands;
