//! The self-update (RC-100-05, RC-100-06): what is known and a check now are `Read` - a check
//! changes no file and sends nothing about the installation; the download and the install are
//! `GuiOnly`: replacing the program is a person's decision after the release notes, and an agent
//! never replaces the program it talks to.

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "update.check",
        class: CommandClass::Read,
        summary: "Check for a newer rCommander now against the signed manifest; changes no file",
    },
    CommandSpec {
        id: "update.download",
        class: CommandClass::GuiOnly,
        summary: "Download the offered version, kept only when its size and SHA-256 match the manifest",
    },
    CommandSpec {
        id: "update.install",
        class: CommandClass::GuiOnly,
        summary: "Install the downloaded version and restart; the old one comes back if the new fails",
    },
    CommandSpec {
        id: "update.status",
        class: CommandClass::Read,
        summary: "Running version, channel, last check and its result, the offer with its notes, download and install state",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "update.status" => GetUpdateStatus {},
            "update.check" => CheckForUpdates {},
            /// The offer's archive, into `<data>/update/download/`.
            "update.download" => DownloadUpdate {},
            /// Refused with `update.queue_busy` while a job runs, waits or is paused.
            "update.install" => InstallUpdate {},
        } }
    };
}
pub(super) use commands;
