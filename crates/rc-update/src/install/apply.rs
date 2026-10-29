//! The updater's run (RC-100-06): what `rcommander apply-update --journal <path>` does, started
//! hidden from a copy of the program in `<data>/update/updater/`.
//!
//! 1. Holds the updater lock for its whole life; waits until no window holds the instance lock
//!    (at most [`INSTANCE_WAIT_SECS`]) - the window ended as `app.quit` ends it.
//! 2. Checks the artifact once more and the program folder (writable, twice the artifact free);
//!    a refusal here changes nothing and starts the old version again.
//! 3. Unpacks from the handle that was hashed and switches (`portable.rs`).
//! 4. Starts the new version (unless the plan says "install when quitting") and waits until it
//!    writes `verified` into the journal, at most the plan's timeout.
//! 5. On a crash, a timeout or a refused proof: ends it, takes the switch back, puts the database
//!    copy back, and starts the old version.
//!
//! Exit codes: `0` the new version is in place, `2` the old one runs again, `1` anything else;
//! the journal says what happened, and the next start shows it.
//!
//! Adapted from rDownloader's `crates/rdownloader/src/updater_cli.rs` at commit 26c8592a6: no
//! service stop, no MSI, no health route - the journal is the answer.

use std::path::Path;
use std::process::Child;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use super::{
    INSTANCE_WAIT_SECS, InstallError, InstanceLock, Journal, Phase, UpdaterLock, durable, portable,
    preflight, process, recover, steps,
};

/// The shortest wait for the new version's proof, whatever the plan says.
const MIN_CONFIRM_SECS: u64 = 5;
/// How often the journal is read while the updater waits.
const POLL: Duration = Duration::from_millis(500);

/// A step's failure: its stable code and what happened.
type Failure = (&'static str, String);

/// Runs the update the journal at `journal_path` hands over. `free_space` measures the volume of
/// a folder (`rc_vfs::disk_space` in the binary); `None` when it does not say.
///
/// # Errors
///
/// When there is no handed-over update or the journal cannot be written; the exit code otherwise.
pub fn run(journal_path: &Path, free_space: &dyn Fn(&Path) -> Option<u64>) -> Result<i32> {
    run_with(
        journal_path,
        free_space,
        Duration::from_secs(INSTANCE_WAIT_SECS),
    )
}

/// [`run`] with the wait for the old window given, for the tests.
pub(crate) fn run_with(
    journal_path: &Path,
    free_space: &dyn Fn(&Path) -> Option<u64>,
    instance_wait: Duration,
) -> Result<i32> {
    let data = journal_path
        .parent()
        .and_then(Path::parent)
        .context("the journal is not inside <data>/update")?
        .to_path_buf();
    let Some(lock) = UpdaterLock::acquire(&data)? else {
        anyhow::bail!("another updater runs for {}", data.display());
    };
    let mut journal = Journal::read(&data)?.context("no update is waiting in this data folder")?;
    anyhow::ensure!(
        journal.phase == Phase::Handed,
        "the update is {} already; only a handed-over one is applied",
        journal.phase.as_str()
    );
    apply(&mut journal, &data, free_space, instance_wait)?;
    tracing::info!(
        phase = journal.phase.as_str(),
        reason = journal.reason.as_deref().unwrap_or(""),
        "the updater ends"
    );
    drop(lock);
    Ok(match journal.phase {
        Phase::Verified | Phase::Switched => 0,
        Phase::RolledBack => 2,
        _ => 1,
    })
}

fn apply(
    journal: &mut Journal,
    data: &Path,
    free_space: &dyn Fn(&Path) -> Option<u64>,
    instance_wait: Duration,
) -> Result<()> {
    if let Err(error) = journal.plan.validate() {
        return journal.end(Phase::Failed, error.code, error.detail);
    }
    if !InstanceLock::wait_until_free(data, instance_wait)? {
        // The old window still runs: nothing changed, and nothing is started.
        return journal.end(
            Phase::Failed,
            "update.app_did_not_end",
            format!(
                "a window still ran after {} seconds",
                instance_wait.as_secs()
            ),
        );
    }
    let install = journal.plan.install_dir.clone();
    let fit = steps::verify_artifact(&journal.plan)
        .and_then(|()| preflight(&install, journal.plan.size, free_space(&install)));
    if let Err(error) = fit {
        return restart_old(journal, error.code, &error.detail);
    }
    if let Err(error) = portable::stage(journal) {
        let _ = durable::remove_any(&journal.staged_dir());
        let code = error
            .downcast_ref::<InstallError>()
            .map_or("update.unpack_failed", |error| error.code);
        return restart_old(journal, code, &format!("{error:#}"));
    }
    if let Err(error) = portable::switch(journal) {
        return take_back(journal, None, "update.switch_failed", format!("{error:#}"));
    }
    if !journal.plan.start_new {
        // "Install when quitting": the next start a person makes proves it.
        return Ok(());
    }
    // Recorded before the start, so a stop now still puts the database copy back.
    journal.new_started = true;
    journal.write()?;
    let mut child = match process::start_program(journal) {
        Ok(child) => child,
        Err(error) => {
            return take_back(journal, None, "update.start_failed", format!("{error:#}"));
        }
    };
    match await_confirmation(data, journal, &mut child) {
        Ok(()) => {
            if let Some(read) = Journal::read(data)? {
                *journal = read;
            }
            tracing::info!(version = %journal.plan.target_version, "the update is installed and proven");
            Ok(())
        }
        Err((code, detail)) => take_back(journal, Some(child), code, detail),
    }
}

/// Waits until the started version writes `verified`, ends, or the plan's time is up.
fn await_confirmation(data: &Path, journal: &Journal, child: &mut Child) -> Result<(), Failure> {
    let limit = Duration::from_secs(journal.plan.confirm_timeout_secs.max(MIN_CONFIRM_SECS));
    let started = Instant::now();
    loop {
        if Journal::read(data)
            .ok()
            .flatten()
            .is_some_and(|read| read.phase == Phase::Verified)
        {
            return Ok(());
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err((
                "update.new_version_exited",
                format!(
                    "{} ended right after its start ({status})",
                    journal.plan.target_version
                ),
            ));
        }
        if started.elapsed() >= limit {
            return Err((
                "update.confirm_timeout",
                format!(
                    "{} did not prove itself within {} seconds",
                    journal.plan.target_version,
                    limit.as_secs()
                ),
            ));
        }
        std::thread::sleep(POLL);
    }
}

/// Nothing was switched: records why and starts the installed version again.
fn restart_old(journal: &mut Journal, code: &'static str, detail: &str) -> Result<()> {
    tracing::error!(
        code,
        detail,
        "the update did not go ahead; the installed version starts again"
    );
    journal.end(Phase::Failed, code, detail)?;
    start_again(journal);
    Ok(())
}

/// The new version failed: ends it, takes the switch back with the database copy, starts the old.
fn take_back(
    journal: &mut Journal,
    new: Option<Child>,
    code: &'static str,
    detail: String,
) -> Result<()> {
    tracing::error!(code, %detail, "the new version is taken back");
    if let Some(mut child) = new {
        let _ = child.kill();
        let _ = child.wait();
    }
    if let Err(error) = recover::take_back(journal, code, &detail) {
        tracing::error!(%error, "the previous version could not be put back; see the manual recovery in docs/development.md");
        return Ok(());
    }
    start_again(journal);
    Ok(())
}

/// Starts the program in the folder again, unless the plan leaves the start to a person.
fn start_again(journal: &Journal) {
    if !journal.plan.start_new {
        return;
    }
    if let Err(error) = process::start_program(journal) {
        tracing::error!(%error, "rCommander could not be started after the update");
    }
}

#[cfg(test)]
#[path = "apply_tests.rs"]
mod tests;
