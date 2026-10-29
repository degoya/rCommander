//! Backing up and restoring rCommander's database (RC-010-12).

use serde::{Deserialize, Serialize};

use super::{CommandClass, CommandSpec};

/// The parameters of `db.backup_to`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupToRequest {
    /// A file, or a folder for a generated name.
    pub path: String,
    /// Replace an existing file; without it an existing file is refused.
    #[serde(default)]
    pub overwrite: bool,
}

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "db.backup",
        class: CommandClass::Write,
        summary: "Back up the database into the backup folder beside it, under a generated name",
    },
    CommandSpec {
        id: "db.backup_to",
        class: CommandClass::GuiOnly,
        summary: "Back up the database to a file or folder a person chose",
    },
    CommandSpec {
        id: "db.restore",
        class: CommandClass::GuiOnly,
        summary: "Check a backup and replace the database with it at the next start",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            /// Into `rc_db::BACKUP_DIR` beside the database, under a generated name: no path, so
            /// MCP can write nowhere else.
            "db.backup" => BackupDatabase,
            "db.backup_to" => BackupDatabaseTo(BackupToRequest),
            /// Checked now, swapped in at the next start, after a backup of the current state.
            "db.restore" => RestoreDatabase { path: String },
        } }
    };
}
pub(super) use commands;

#[cfg(test)]
#[path = "backup_tests.rs"]
mod tests;
