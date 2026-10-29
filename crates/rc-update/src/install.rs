//! Installing a verified update and taking it back (RC-100-06): the files of the switch, the
//! journal that makes it recoverable, and what a start does with an interrupted one.
//!
//! **Who does what.** The window verifies and downloads the artifact (`download.rs`), writes the
//! database copy (`rc_db` backup), writes the [`Journal`] in phase `handed` and starts the
//! updater: a copy of its own executable outside the program folder ([`process::launch_updater`]),
//! because a running `.exe` cannot be replaced on Windows. Then the window ends as `app.quit`
//! does. The updater (`rcommander apply-update`, [`apply::run`]) holds [`UpdaterLock`] for its
//! whole life, waits until no window holds [`InstanceLock`] any more, switches the files, starts
//! the new version and waits until it writes `verified` into the journal once its database is
//! migrated and its window is open. A failure takes the switch back, puts the database copy in
//! place and starts the old version.
//!
//! **The portable switch** ([`portable`]) unpacks into `<install>/.update-<version>/`, moves each
//! top-level entry of the new archive that exists now into `<install>/.previous/`, then the new
//! one into its place; nothing else in the folder moves. Leftovers go through `<install>/.trash/`
//! first ([`trash`]): what a still-running `rcommander mcp` holds stays there for a later sweep.
//!
//! Copied and adapted from rDownloader's `crates/rd-update/src/install/` at commit 26c8592a6: no
//! MSI, no control token and no HTTP health check, an instance lock instead of a service stop.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

pub mod apply;
mod durable;
mod journal;
mod journal_check;
mod locks;
pub mod portable;
pub mod process;
pub mod recover;
pub mod steps;
pub mod trash;

pub use journal::{Journal, Phase, Plan};
pub use locks::{InstanceLock, UpdaterLock, updater_running};

/// Below the data directory: what an update keeps between the window and the updater.
pub const UPDATE_DIR: &str = "update";
/// Inside [`UPDATE_DIR`].
pub const JOURNAL_FILE: &str = "journal.json";
/// Inside [`UPDATE_DIR`]: held by the updater for as long as it runs.
pub const LOCK_FILE: &str = "updater.lock";
/// Inside [`UPDATE_DIR`]: held, shared, by every window for as long as it runs.
pub const INSTANCE_LOCK_FILE: &str = "instance.lock";
/// Inside [`UPDATE_DIR`]: the verified downloads.
pub const DOWNLOAD_DIR: &str = "download";
/// Inside [`UPDATE_DIR`]: the database copies written before an update.
pub const PRE_UPDATE_DIR: &str = "pre-update";
/// Inside [`UPDATE_DIR`]: a journal [`Journal::check`] refused, kept for a person to read.
pub const REJECTED_FILE: &str = "journal.rejected.json";
/// Beside the program files: the entries the switch replaced.
pub const PREVIOUS_DIR: &str = ".previous";
/// How long the updater waits for the old window to end.
pub const INSTANCE_WAIT_SECS: u64 = 60;
/// How long the updater waits for the new version's proof.
pub const DEFAULT_CONFIRM_TIMEOUT_SECS: u64 = 90;
/// The hidden subcommand the updater runs as.
pub const APPLY_COMMAND: &str = "apply-update";
/// Honoured by a debug build only: the new version refuses to prove itself, so a test shows the
/// way back without publishing a broken release.
pub const TEST_FAIL_START_ENV: &str = "RC_UPDATE_TEST_FAIL_START";

/// The names below the program folder and `<data>/update` that only the update writes: guarded
/// targets for MCP's write tools (RC-080-09).
pub const GUARDED_NAMES: &[&str] = &[".update-", ".previous", ".trash", ".failed-"];

/// `<data>/update`.
#[must_use]
pub fn update_dir(data: &Path) -> PathBuf {
    data.join(UPDATE_DIR)
}

/// Why an install was refused or failed: a stable code the window translates, and what happened
/// in words for the log.
#[derive(Debug, thiserror::Error)]
#[error("{detail}")]
pub struct InstallError {
    pub code: &'static str,
    pub detail: String,
}

impl InstallError {
    #[must_use]
    pub fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

/// Every code an install, its updater or a start's recovery records, for the i18n test that each
/// has a text the old version shows at its start.
pub const INSTALL_CODES: &[&str] = &[
    "update.plan_invalid",
    "update.journal_invalid",
    "update.install_dir_not_writable",
    "update.not_enough_space",
    "update.digest_mismatch",
    "update.download_failed",
    "update.archive_unknown",
    "update.archive_incomplete",
    "update.unpack_failed",
    "update.switch_failed",
    "update.start_failed",
    "update.new_version_exited",
    "update.not_confirmed",
    "update.confirm_timeout",
    "update.app_did_not_end",
    "update.updater_did_not_start",
    "update.interrupted",
    "update.rollback_failed",
];

/// Refuses an install that cannot succeed: a program folder this process may not write, or a
/// volume with less than twice the artifact free (`free` as the caller measured it; `None` when
/// the volume does not say, which is not refused).
///
/// # Errors
///
/// `update.install_dir_not_writable` or `update.not_enough_space`.
pub fn preflight(
    install_dir: &Path,
    artifact_size: u64,
    free: Option<u64>,
) -> Result<(), InstallError> {
    let probe = install_dir.join(".update-probe");
    let written = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&probe)
        .and_then(|mut file| file.write_all(b"probe"));
    let _ = fs::remove_file(&probe);
    if let Err(error) = written {
        return Err(InstallError::new(
            "update.install_dir_not_writable",
            format!("{} cannot be written: {error}", install_dir.display()),
        ));
    }
    let required = artifact_size.saturating_mul(2);
    match free {
        Some(free) if free < required => Err(InstallError::new(
            "update.not_enough_space",
            format!(
                "{} has {free} bytes free, the update needs {required}",
                install_dir.display()
            ),
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
pub(crate) mod fixture;

#[cfg(test)]
#[path = "install_tests.rs"]
mod tests;
