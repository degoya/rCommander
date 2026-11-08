//! Packing and unpacking archives as jobs of the operations queue (RC-030-03, RC-030-04), and
//! changing one - files added, entries removed (RC-110-05).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "archive.add",
        class: CommandClass::Write,
        summary: "Add files and folders to an existing zip, 7z or tar archive (into a folder inside it) as a queued job; the archive is rewritten beside itself and replaced when complete",
    },
    CommandSpec {
        id: "archive.delete",
        class: CommandClass::Destructive,
        summary: "Remove entries from a zip, 7z or tar archive for good (no recycle bin) as a queued job; the archive is rewritten beside itself and replaced when complete",
    },
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
            /// F5 (F6 with `delete_sources`) with an archive in the target panel (RC-110-05):
            /// the sources into the archive the other panel shows, at the folder it shows. From a
            /// key without `target` the window first opens the pack dialog in its add mode.
            "archive.add" => ArchiveAdd {
                #[serde(default)]
                panel: Option<PanelId>,
                /// Absolute, or relative to the panel's directory. Empty: the selection, else the
                /// cursor.
                #[serde(default)]
                sources: Vec<String>,
                /// Where inside which archive: a location inside an archive (`C:\a.zip\docs`)
                /// or an archive file (its root); `None` is the other panel's location, which
                /// must lie inside an archive.
                #[serde(default)]
                target: Option<String>,
                #[serde(default)]
                settings: rc_ops::UpdateSettings,
                #[serde(default)]
                options: JobOptions,
            },
            /// F8 or Del inside an archive (RC-110-05): the entries go for good.
            "archive.delete" => ArchiveDelete {
                #[serde(default)]
                panel: Option<PanelId>,
                /// Names in the panel's directory inside an archive. Empty: the selection, else
                /// the cursor.
                #[serde(default)]
                entries: Vec<String>,
                #[serde(default)]
                options: JobOptions,
            },
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
