//! The errors of a backup and a restore, each with a stable code.

use std::{io, path::PathBuf};

/// What went wrong backing the database up or restoring a backup.
///
/// [`BackupError::code`] is the stable form the GUI shows a message under and MCP returns. Paths
/// are the user's own; no message carries database content.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BackupError {
    /// The target exists and overwriting it was not asked for; it is unchanged.
    #[error("{} exists already; choose another name or allow overwriting it", path.display())]
    TargetExists { path: PathBuf },
    /// The target is the database file itself.
    #[error("{} is the database itself; a backup goes into another file", path.display())]
    TargetIsDatabase { path: PathBuf },
    /// The target's folder is missing or not writable, or the finished file cannot take the
    /// target's name.
    #[error("cannot write the backup to {}: {source}", path.display())]
    Target {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// SQLite takes file names as UTF-8; this one is not.
    #[error("the path {} is not valid Unicode", path.display())]
    PathNotUnicode { path: PathBuf },
    /// There is no database file to back up.
    #[error("there is no database file at {}", path.display())]
    NoDatabase { path: PathBuf },
    /// A database in memory has no file and no folder for backups.
    #[error("a database in memory has no file to back up beside or to restore into")]
    InMemory,
    /// The copy did not pass `PRAGMA integrity_check`; it was removed.
    #[error("the copy failed its integrity check: {detail}")]
    Unverified { detail: String },
    /// SQLite failed while copying.
    #[error("backup: {0}")]
    Sqlite(#[from] sqlx::Error),
    /// The caller stopped waiting before the copy was complete; nothing was kept.
    #[error("the backup was stopped before it was complete")]
    Interrupted,
    /// The file to restore cannot be read as a sound SQLite database.
    #[error("{} is not a sound SQLite database: {detail}", path.display())]
    RestoreNotDatabase { path: PathBuf, detail: String },
    /// The file is a SQLite database, but not one of rCommander's - or its schema is not this
    /// build's.
    #[error("{} is not an rCommander database this version can read", path.display())]
    RestoreForeign { path: PathBuf },
    /// The file comes from a newer rCommander, whose schema this build does not know.
    #[error(
        "{} comes from a newer rCommander (schema {found}; this version knows up to {known})",
        path.display()
    )]
    RestoreNewer {
        path: PathBuf,
        found: i64,
        known: i64,
    },
    /// The staged backup cannot replace the database file (another program holds it open).
    #[error("cannot put the restored database in place: {source}")]
    RestoreSwap {
        #[source]
        source: io::Error,
    },
    /// The file staged beside the database has no matching mark of rCommander's restore or fails
    /// its check again: it was set aside, the database is unchanged (RC-080-09).
    #[error(
        "the staged restore was not put in place and was set aside as {}: {reason}",
        set_aside.display()
    )]
    RestoreRejected { set_aside: PathBuf, reason: String },
}

impl BackupError {
    /// Every code.
    pub const ALL_CODES: &[&str] = &[
        "backup_target_exists",
        "backup_target_is_database",
        "backup_target_unwritable",
        "backup_path_invalid",
        "backup_no_database",
        "backup_in_memory",
        "backup_unverified",
        "backup_failed",
        "backup_interrupted",
        "restore_not_database",
        "restore_not_rcommander",
        "restore_newer",
        "restore_swap_failed",
        "restore_rejected",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::TargetExists { .. } => "backup_target_exists",
            Self::TargetIsDatabase { .. } => "backup_target_is_database",
            Self::Target { .. } => "backup_target_unwritable",
            Self::PathNotUnicode { .. } => "backup_path_invalid",
            Self::NoDatabase { .. } => "backup_no_database",
            Self::InMemory => "backup_in_memory",
            Self::Unverified { .. } => "backup_unverified",
            Self::Sqlite(_) => "backup_failed",
            Self::Interrupted => "backup_interrupted",
            Self::RestoreNotDatabase { .. } => "restore_not_database",
            Self::RestoreForeign { .. } => "restore_not_rcommander",
            Self::RestoreNewer { .. } => "restore_newer",
            Self::RestoreSwap { .. } => "restore_swap_failed",
            Self::RestoreRejected { .. } => "restore_rejected",
        }
    }
}
