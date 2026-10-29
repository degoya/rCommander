//! `cmdline.run`: Enter in the command line under the panels. `cd` and a drive move the panel
//! ([`crate::command_line::parse`]); any other line starts in the system's shell in the panel's
//! folder - `cmd /s /c` on Windows (`/k` with Shift+Enter, so the window stays open), `sh -c`
//! elsewhere - in a console window of its own, as Total Commander does; nothing waits for it.
//! Only a local folder runs a command: inside an archive or on a server there is no folder for
//! the shell (the terminal panel and `ssh_exec` do that).

#[cfg(windows)]
mod windows;

use std::path::Path;

use crate::{
    Engine, EngineError, Outcome, PanelId,
    command_line::{Parsed, check, parse, runs_in},
};

impl Engine {
    /// `cmdline.run`: `line` for panel `panel`, `keep_open` for Shift+Enter.
    pub(crate) async fn run_command_line(
        &self,
        panel: Option<PanelId>,
        line: &str,
        keep_open: bool,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        match parse(check(line)?) {
            Parsed::Stay => Ok(Outcome::Panel(self.snapshot(Some(id)))),
            Parsed::Drive(letter) => {
                self.change_drive_to(Some(id), &letter.to_string(), true)
                    .await
            }
            Parsed::Cd(target) => self.navigate(Some(id), &target).await,
            Parsed::Run(command) => {
                let folder = self.state().panel(id).path.clone();
                if !runs_in(&folder) {
                    return Err(EngineError::CommandLineLocalOnly);
                }
                start(command, &folder.path, keep_open)
                    .map_err(|source| EngineError::CommandLineFailed { source })?;
                Ok(Outcome::Panel(self.snapshot(Some(id))))
            }
        }
    }
}

/// The arguments of `cmd.exe` for `line`: `/s` takes the outer quotes off exactly, so the line
/// reaches cmd as typed, quotes and all; `/k` keeps the window open after it.
#[cfg(any(windows, test))]
pub(crate) fn cmd_arguments(line: &str, keep_open: bool) -> String {
    let after = if keep_open { 'k' } else { 'c' };
    format!("/s /{after} \"{line}\"")
}

/// The shell started for `line`, not yet in any folder.
#[cfg(windows)]
fn shell_command(line: &str, keep_open: bool) -> std::process::Command {
    windows::shell_command(line, keep_open)
}

/// The shell started for `line`; a window that stays open needs a terminal of the desktop, which
/// comes with the Linux and macOS builds.
#[cfg(not(windows))]
fn shell_command(line: &str, _keep_open: bool) -> std::process::Command {
    let mut command = std::process::Command::new("/bin/sh");
    command.arg("-c").arg(line);
    command
}

/// Starts `line` in `folder` and leaves it running; a background task reaps it when it ends.
#[cfg(not(test))]
fn start(line: &str, folder: &Path, keep_open: bool) -> std::io::Result<()> {
    let mut command = shell_command(line, keep_open);
    command.current_dir(folder);
    let mut child = tokio::process::Command::from(command).spawn()?;
    tokio::spawn(async move {
        // How the command ended is the console window's to show.
        let _ = child.wait().await;
    });
    Ok(())
}

/// The tests start nothing: the line, its folder and `keep_open` are recorded.
#[cfg(test)]
fn start(line: &str, folder: &Path, keep_open: bool) -> std::io::Result<()> {
    tests::STARTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((line.to_owned(), folder.to_path_buf(), keep_open));
    Ok(())
}

#[cfg(test)]
#[path = "command_line_tests.rs"]
mod tests;
