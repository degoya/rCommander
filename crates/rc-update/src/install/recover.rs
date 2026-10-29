//! What a start does with the update its journal records (RC-100-06), **before** it opens the
//! database: the one place an update stopped by a crash, a killed updater or a power cut ends.
//!
//! Nothing here acts while the updater runs (its lock is held): the start it made is the new
//! version it waits for. Nothing acts either when this executable is not the one in the journal's
//! program folder - a build from source sharing the data, say.
//!
//! | Journal | The start |
//! | --- | --- |
//! | `handed`, `staged` | nothing live changed: removes the staging, records `update.interrupted` |
//! | `switching`, `rolling_back` | takes the switch back, the database copy too if the new version had started, records why, and restarts as the old version if this is not it |
//! | `switched`, this is the new version | the first such start may prove it ([`confirm_started`]); a second unproven one takes it back |
//! | `verified`, `rolled_back`, `failed` | removes the leftovers, once - except after `update.rollback_failed`, whose files the manual recovery needs |
//!
//! Copied and adapted from rDownloader's `crates/rd-update/src/install/recover.rs` at commit
//! 26c8592a6: no MSI; the new version proves itself while the updater still waits.

use std::path::{Path, PathBuf};

use anyhow::Result;

use super::{
    Journal, Phase, TEST_FAIL_START_ENV, durable::remove_any, portable, steps, trash,
    updater_running,
};

/// What the start does next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recovery {
    /// Start as usual.
    Continue,
    /// The program files were put back to the version before the update, and this process is
    /// the newer one: start this executable with the same arguments instead and end.
    Restart(PathBuf),
}

/// Ends or continues the update the journal in `data` records; see the module documentation.
///
/// # Errors
///
/// When the journal is unreadable or a roll-back fails; the start must not go on then, because
/// the program folder may hold two versions at once.
pub fn recover_at_start(data: &Path, executable: &Path, running_version: &str) -> Result<Recovery> {
    let Some(mut journal) = Journal::read(data)? else {
        return Ok(Recovery::Continue);
    };
    if updater_running(data) || !runs_from(&journal, executable) {
        return Ok(Recovery::Continue);
    }
    let is_new = running_version == journal.plan.target_version;
    match journal.phase {
        Phase::Handed | Phase::Staged => {
            let _ = remove_any(&journal.staged_dir());
            journal.end(
                Phase::Failed,
                "update.interrupted",
                "the updater ended before it changed the program",
            )?;
            Ok(Recovery::Continue)
        }
        Phase::Switching | Phase::RollingBack => {
            tracing::warn!(target = %journal.plan.target_version, "an interrupted update is taken back");
            // A roll-back the updater began keeps its reason; a switch it never finished is the
            // interruption itself.
            let (code, detail) = match (journal.phase, journal.reason.clone()) {
                (Phase::RollingBack, Some(reason)) => {
                    (reason, journal.detail.clone().unwrap_or_default())
                }
                _ => (
                    "update.interrupted".to_owned(),
                    "the update was interrupted while the program files were being switched"
                        .to_owned(),
                ),
            };
            take_back(&mut journal, &code, &detail)?;
            Ok(restart_unless(&journal, running_version))
        }
        Phase::Switched if is_new => {
            if journal.start_attempts == 0 {
                journal.start_attempts += 1;
                journal.write()?;
                return Ok(Recovery::Continue);
            }
            tracing::error!(target = %journal.plan.target_version, "the new version's first start never proved itself; the previous version is put back");
            journal.new_started = true;
            take_back(
                &mut journal,
                "update.not_confirmed",
                "the new version did not prove itself after it was started",
            )?;
            Ok(restart_unless(&journal, running_version))
        }
        Phase::Switched => Ok(Recovery::Continue),
        Phase::Verified | Phase::RolledBack | Phase::Failed => {
            trash::sweep(&journal.plan.install_dir);
            let keep = journal.reason.as_deref() == Some("update.rollback_failed");
            let proven = journal.phase != Phase::Verified || is_new;
            if !journal.cleaned && !keep && proven {
                steps::clean_up(&journal);
                journal.cleaned = true;
                journal.write()?;
            }
            Ok(Recovery::Continue)
        }
    }
}

/// The new version's database is migrated and its window is open: an update waiting for its
/// proof is proven by that - while its updater still waits, or at the first start a person made
/// after "install when quitting". Returns whether this proved one.
///
/// A debug build with [`TEST_FAIL_START_ENV`] set refuses, so the way back can be tested.
///
/// # Errors
///
/// When the journal cannot be read or written.
pub fn confirm_started(data: &Path, running_version: &str) -> Result<bool> {
    let Some(mut journal) = Journal::read(data)? else {
        return Ok(false);
    };
    if journal.phase != Phase::Switched || journal.plan.target_version != running_version {
        return Ok(false);
    }
    if refuses_for_test() {
        tracing::warn!("{TEST_FAIL_START_ENV} is set: this start does not prove the update");
        return Ok(false);
    }
    journal.advance(Phase::Verified)?;
    tracing::info!(
        version = running_version,
        "the update is proven by this start"
    );
    Ok(true)
}

/// Whether this is a debug build told to refuse the proof.
fn refuses_for_test() -> bool {
    cfg!(debug_assertions) && std::env::var_os(TEST_FAIL_START_ENV).is_some()
}

/// Whether the copies kept for taking the last update back may thin out (RC-100-07): no update
/// is recorded, or the last one is `verified` for the version running now and no updater runs.
/// An update still waiting for its proof, rolled back or failed keeps them all, and so does a
/// journal that cannot be read.
#[must_use]
pub fn update_proven(data: &Path, running_version: &str) -> bool {
    if updater_running(data) {
        return false;
    }
    match Journal::read(data) {
        Ok(None) => true,
        Ok(Some(journal)) => {
            journal.phase == Phase::Verified && journal.plan.target_version == running_version
        }
        Err(_) => false,
    }
}

/// Rolls the switch back, the database copy with it if the new version ran, and ends the journal;
/// a failure ends it as `update.rollback_failed` and is returned.
pub(crate) fn take_back(journal: &mut Journal, code: &str, detail: &str) -> Result<()> {
    journal.phase = Phase::RollingBack;
    journal.write()?;
    let result = portable::roll_back(journal).and_then(|()| {
        if journal.new_started {
            steps::restore_database(&journal.plan)?;
        }
        Ok(())
    });
    match result {
        Ok(()) => journal.end(Phase::RolledBack, code, detail),
        Err(error) => {
            journal.end(
                Phase::Failed,
                "update.rollback_failed",
                format!("{detail}; taking it back failed: {error:#}"),
            )?;
            Err(error)
        }
    }
}

fn restart_unless(journal: &Journal, running_version: &str) -> Recovery {
    if running_version == journal.plan.from_version {
        Recovery::Continue
    } else {
        Recovery::Restart(journal.executable())
    }
}

/// Whether `executable` is the program in the journal's folder.
fn runs_from(journal: &Journal, executable: &Path) -> bool {
    let canonical = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    canonical(executable) == canonical(&journal.executable())
}

#[cfg(test)]
#[path = "recover_tests.rs"]
mod tests;
