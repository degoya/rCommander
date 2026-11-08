//! Starts through Windows' shell (RC-100-86): `ShellExecuteExW`, with the verb `runas` for a
//! start as administrator (`*` in front of a command line, an own command or a Start menu entry
//! whose `elevated` is set), and without one for a program that opens minimized or maximized,
//! which `std::process::Command` cannot ask for. The program is always named by its full path -
//! the shell searches nothing, so never the panel's folder. Windows' own question (UAC) is the
//! confirmation: there is no setting to skip it, and "no" there is no error
//! ([`ShellStarted::Cancelled`], a line in the status bar).
//!
//! The command line's `*<line>` runs `cmd.exe` from the system folder as administrator, `/s /c`
//! (`/k` with Shift+Enter), with `pushd "<folder>" &&` in front, since an elevated cmd does not
//! take over the folder it was started in. A folder on a mapped network drive goes as its UNC
//! path ([`unc_folder`]): the elevated process does not see the letters of the person's session.
//! A leading `cmd` in the line is the one from the system folder too, never one in the panel's
//! folder. Outside Windows nothing starts: `run_as_admin_unsupported` (`pkexec` and `sudo` would
//! need a terminal or a polkit agent and a password outside the vault).

#[cfg(all(windows, not(test)))]
mod windows;

use std::path::{Path, PathBuf};

use crate::{
    Drive, DriveKind, Engine, EngineError, EngineEvent, UserCommandError, user_commands::Show,
};

/// One start through the shell.
#[cfg_attr(
    any(not(windows), test),
    allow(
        dead_code,
        reason = "read and made where a start goes through Windows' shell; elsewhere and in the \
                  tests nothing does"
    )
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShellStart {
    /// The program's full path.
    pub program: PathBuf,
    /// Its parameters as one text, as Windows programs read them.
    pub params: String,
    /// Where it starts; `None` leaves the shell's.
    pub folder: Option<PathBuf>,
    pub show: Show,
    /// With the verb `runas`.
    pub elevated: bool,
}

/// What a start through the shell did.
#[cfg_attr(
    any(not(windows), test),
    allow(
        dead_code,
        reason = "read and made where a start goes through Windows' shell; elsewhere and in the \
                  tests nothing does"
    )
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShellStarted {
    Started,
    /// The person said no to Windows' question.
    Cancelled,
}

/// Starts `start` through the shell, off the async threads (the call waits while Windows asks).
///
/// # Errors
///
/// Windows refused the start ([`UserCommandError::RunAsAdminFailed`] with its code).
#[cfg(all(windows, not(test)))]
pub(crate) async fn shell_start(start: ShellStart) -> Result<ShellStarted, UserCommandError> {
    use crate::recover::LogPanic as _;
    tokio::task::spawn_blocking(move || windows::shell_execute(&start))
        .await
        .log_panic("engine.shell_start")
        .map_err(|error| UserCommandError::Failed {
            source: std::io::Error::other(error),
        })?
        .map_err(|code| UserCommandError::RunAsAdminFailed { code })
}

/// Outside Windows there is no shell start: nothing starts.
///
/// # Errors
///
/// Always [`UserCommandError::RunAsAdminUnsupported`].
#[cfg(all(not(windows), not(test)))]
pub(crate) async fn shell_start(_start: ShellStart) -> Result<ShellStarted, UserCommandError> {
    Err(UserCommandError::RunAsAdminUnsupported)
}

/// The tests start nothing: the start is recorded in [`STARTED`].
#[cfg(test)]
pub(crate) async fn shell_start(start: ShellStart) -> Result<ShellStarted, UserCommandError> {
    use crate::recover::LockOrRecover as _;
    STARTED.lock_or_recover("elevate.started").push(start);
    Ok(ShellStarted::Started)
}

/// `folder` as the elevated process can reach it: a folder on a mapped network drive of `drives`
/// (`Z:\work`) as its UNC path (`\\server\share\work`); any other folder as it is.
#[must_use]
pub(crate) fn unc_folder(folder: &str, drives: &[Drive]) -> String {
    drives
        .iter()
        .filter(|drive| drive.kind == DriveKind::Network)
        .find_map(|drive| {
            let remote = drive.remote.as_deref()?;
            let letter = drive.id.get(..2).filter(|id| id.ends_with(':'))?;
            let head = folder.get(..2)?;
            if !head.eq_ignore_ascii_case(letter) {
                return None;
            }
            let rest = folder[2..].trim_start_matches(['\\', '/']);
            let remote = remote.trim_end_matches(['\\', '/']);
            Some(if rest.is_empty() {
                remote.to_owned()
            } else {
                format!("{remote}\\{rest}")
            })
        })
        .unwrap_or_else(|| folder.to_owned())
}

/// The parameters of the elevated `cmd.exe` for `line` in `folder`: `/s` takes the outer quotes
/// off exactly, `pushd` goes to the folder first, `/k` keeps the window open. A leading `cmd`
/// (or `cmd.exe`) of the line is `cmd_path`, the system folder's.
#[must_use]
pub(crate) fn elevated_cmd_arguments(
    folder: &str,
    line: &str,
    keep_open: bool,
    cmd_path: &Path,
) -> String {
    let after = if keep_open { 'k' } else { 'c' };
    let line = line.trim();
    let (first, rest) = line
        .split_once(char::is_whitespace)
        .map_or((line, ""), |(first, rest)| (first, rest));
    let line = if first.eq_ignore_ascii_case("cmd") || first.eq_ignore_ascii_case("cmd.exe") {
        let cmd = format!("\"{}\"", cmd_path.display());
        if rest.is_empty() {
            cmd
        } else {
            format!("{cmd} {rest}")
        }
    } else {
        line.to_owned()
    };
    format!("/s /{after} \"pushd \"{folder}\" && {line}\"")
}

/// `cmd.exe` in the system folder (`SystemRoot` only when absolute, RC-080-65).
fn system_cmd() -> PathBuf {
    rc_vfs::system32_from_env().join("cmd.exe")
}

impl Engine {
    /// The command line's `*<line>` (RC-100-86): `cmd.exe` from the system folder as
    /// administrator in `folder`; Windows only.
    ///
    /// # Errors
    ///
    /// [`UserCommandError::RunAsAdminUnsupported`] outside Windows, or Windows refused the start.
    pub(crate) async fn run_elevated_line(
        &self,
        line: &str,
        folder: &Path,
        keep_open: bool,
    ) -> Result<(), EngineError> {
        if !cfg!(windows) {
            return Err(UserCommandError::RunAsAdminUnsupported.into());
        }
        let drives = self.inner.drives.list(false).await;
        let folder = unc_folder(&folder.to_string_lossy(), &drives);
        let cmd = system_cmd();
        let params = elevated_cmd_arguments(&folder, line, keep_open, &cmd);
        let started = shell_start(ShellStart {
            program: cmd,
            params,
            folder: None,
            show: Show::Normal,
            elevated: true,
        })
        .await?;
        if started == ShellStarted::Cancelled {
            self.publish(EngineEvent::RunAsAdminCancelled);
        }
        Ok(())
    }
}

/// What the tests started as administrator, in order.
#[cfg(test)]
pub(crate) static STARTED: std::sync::Mutex<Vec<ShellStart>> = std::sync::Mutex::new(Vec::new());

#[cfg(test)]
#[path = "elevate_tests.rs"]
mod tests;
