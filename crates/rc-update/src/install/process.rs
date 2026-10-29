//! The processes of an update (RC-100-06): the updater the window starts, and the version the
//! updater starts. Both outlive their starter and open no console window (`rc_process`'s
//! [`Detached`] and [`NoConsoleWindow`]); a job that refuses a breakaway gets the program inside
//! it, as any other child would be.
//!
//! Copied and adapted from rDownloader's `crates/rd-update/src/install/process.rs` at commit
//! 26c8592a6: no service, no MSI, no process ending by name.

use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::{Child, Command, Stdio};

use anyhow::{Context, Result};
use rc_process::{Detached as _, NoConsoleWindow as _};

use super::{APPLY_COMMAND, Journal, durable::remove_any, update_dir};

/// Inside `<data>/update`: the copy of the executable the updater runs from.
pub const UPDATER_DIR: &str = "updater";
/// Inside `<data>/update`: what the updater writes to its output.
pub const UPDATER_LOG: &str = "updater.log";

/// The updater's file name: not `rcommander(.exe)`, so nothing that looks for the program by its
/// image name takes the updater for it.
#[must_use]
pub fn updater_file_name() -> &'static str {
    if cfg!(windows) {
        "rcommander-updater.exe"
    } else {
        "rcommander-updater"
    }
}

/// Starts `program` in `cwd` with `args`, detached, its output appended to `log` (or dropped).
///
/// # Errors
///
/// When the log cannot be opened or the program cannot be started.
pub fn spawn_detached<S: AsRef<OsStr>>(
    program: &Path,
    args: &[S],
    cwd: &Path,
    log: Option<&Path>,
) -> Result<Child> {
    let output = |log: Option<&Path>| -> Result<Stdio> {
        match log {
            Some(path) => Ok(Stdio::from(
                fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .with_context(|| format!("open {}", path.display()))?,
            )),
            None => Ok(Stdio::null()),
        }
    };
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(output(log)?)
        .stderr(output(log)?)
        .no_console_window()
        .detached(true);
    match command.spawn() {
        Ok(child) => Ok(child),
        // A job that allows no breakaway: started inside it.
        Err(error) if cfg!(windows) && error.raw_os_error() == Some(5) => {
            command.detached(false);
            command
                .spawn()
                .with_context(|| format!("start {}", program.display()))
        }
        Err(error) => Err(error).with_context(|| format!("start {}", program.display())),
    }
}

/// Copies the running executable to `<data>/update/updater/` and starts the copy as
/// `apply-update --journal <journal>`: a program cannot replace its own running file on Windows,
/// and outside the program folder the copy is never part of what the switch moves.
///
/// # Errors
///
/// When the copy or the start fails; the journal then still says `handed`, and the window ends
/// it as `update.updater_did_not_start`.
pub fn launch_updater(journal: &Journal) -> Result<()> {
    let current = std::env::current_exe().context("locate the running executable")?;
    let update = update_dir(&journal.plan.data_dir);
    let directory = update.join(UPDATER_DIR);
    remove_any(&directory)?;
    fs::create_dir_all(&directory).with_context(|| format!("create {}", directory.display()))?;
    let copy = directory.join(updater_file_name());
    fs::copy(&current, &copy)
        .with_context(|| format!("copy {} to {}", current.display(), copy.display()))?;
    let journal_path = Journal::path(&journal.plan.data_dir);
    let args = [
        OsStr::new(APPLY_COMMAND),
        OsStr::new("--journal"),
        journal_path.as_os_str(),
    ];
    // The data folder as its working folder: the program folder must stay free to change.
    spawn_detached(&copy, &args, &update, Some(&update.join(UPDATER_LOG)))?;
    Ok(())
}

/// Starts the program of the journal's folder with the window's arguments, the program folder as
/// its working folder.
///
/// # Errors
///
/// When the program cannot be started.
pub fn start_program(journal: &Journal) -> Result<Child> {
    spawn_detached(
        &journal.executable(),
        journal.plan.args.as_slice(),
        &journal.plan.install_dir,
        None,
    )
}
