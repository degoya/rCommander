//! Starting an own command's program (RC-100-86): without a shell, the program and its
//! arguments given directly (`std::process::Command`) - on Windows the parameters as one text
//! (`raw_arg`, Windows programs read their own command line), elsewhere one argument per word.
//! A program named without a folder is found on the search path, never in the panel's folder
//! (the panel's folder is only where it starts). A console program gets its console window, as
//! in Total Commander.
//!
//! Windows runs a batch file (`.bat`, `.cmd`) through cmd, which reads `% ^ & | < > "` as its own
//! syntax: a value put into such a program's parameters that holds one refuses the start
//! ([`batch_values_are_safe`], Rust's own BatBadBut rule for `arg`). The check reads the program
//! as found ([`resolve_checked`]) - a bare `foo` that the search path turns into `foo.bat` is a
//! batch file too, `FOO.BAT.` as well (Windows drops trailing dots and blanks) - and the program
//! starts by that very path, so the check and the start never disagree. A start as administrator, or
//! one minimized or maximized, goes through the shell (`commands/elevate.rs`) with the program
//! found first, so the shell searches nothing either; outside Windows a start as administrator is
//! refused and nothing starts.

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use super::{Show, UserCommandError};
use crate::EngineError;

/// The characters cmd reads as syntax in a batch file's arguments.
const BATCH_SYNTAX: &[char] = &['%', '^', '&', '|', '<', '>', '"'];

/// One program to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Launch {
    /// The program as written, `%VAR%` filled in.
    pub program: String,
    /// Windows: the parameters as one text (none when empty). Elsewhere: one per argument.
    pub args: Vec<String>,
    /// Where it starts; `None` leaves rCommander's.
    pub folder: Option<PathBuf>,
    pub show: Show,
    pub elevated: bool,
    /// Every value put into `args` (names, folders, list files), for the batch-file check.
    pub values: Vec<String>,
}

/// What a start did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Started {
    Started,
    /// The person said no to Windows' question for a start as administrator.
    #[cfg_attr(
        test,
        allow(dead_code, reason = "the tests record their starts, none is cancelled")
    )]
    Cancelled,
}

/// Whether `name` (a program as written or as found) is a batch file: it ends in `.bat` or `.cmd`
/// in any case, after the quotes around it and the trailing dots and blanks Windows ignores.
#[must_use]
pub fn is_batch_file(name: &str) -> bool {
    let lower = name
        .trim()
        .trim_matches('"')
        .trim_end_matches(['.', ' '])
        .to_ascii_lowercase();
    lower.ends_with(".bat") || lower.ends_with(".cmd")
}

/// Whether `values` may go into the parameters of `program`: always, unless the program is a
/// batch file ([`is_batch_file`]) and a value holds one of `% ^ & | < > "`.
#[must_use]
pub fn batch_values_are_safe(program: &str, values: &[String]) -> bool {
    !is_batch_file(program) || values.iter().all(|value| !value.contains(BATCH_SYNTAX))
}

/// `program` found on `search_path` ([`find_program`]) and checked as found: a batch file, by the
/// name as written or as found, gets no value with cmd's syntax.
///
/// # Errors
///
/// [`UserCommandError::UnsafeArgument`], or [`UserCommandError::ProgramNotFound`].
pub fn resolve_checked(
    program: &str,
    search_path: Option<&OsStr>,
    extensions: &[String],
    values: &[String],
) -> Result<PathBuf, UserCommandError> {
    if !batch_values_are_safe(program, values) {
        return Err(UserCommandError::UnsafeArgument);
    }
    let found =
        find_program(program, search_path, extensions).ok_or(UserCommandError::ProgramNotFound)?;
    if !batch_values_are_safe(&found.to_string_lossy(), values) {
        return Err(UserCommandError::UnsafeArgument);
    }
    Ok(found)
}

/// [`resolve_checked`] on this process's search path.
fn resolve(launch: &Launch) -> Result<PathBuf, UserCommandError> {
    resolve_checked(
        &launch.program,
        std::env::var_os("PATH").as_deref(),
        &program_extensions(),
        &launch.values,
    )
}

/// `program` found: a path with a folder as it is, a bare name in the folders of `search_path`
/// (each with the `extensions` when it has none of them; on Windows `PATHEXT`'s). Never the
/// panel's folder, nor the current one.
#[must_use]
pub fn find_program(
    program: &str,
    search_path: Option<&OsStr>,
    extensions: &[String],
) -> Option<PathBuf> {
    let program = program.trim().trim_matches('"');
    if program.is_empty() {
        return None;
    }
    let as_written = Path::new(program);
    if program.contains(['/', '\\']) || as_written.is_absolute() {
        return as_written.is_file().then(|| as_written.to_path_buf());
    }
    let lower = program.to_ascii_lowercase();
    let has_extension = extensions
        .iter()
        .any(|extension| lower.ends_with(&extension.to_ascii_lowercase()));
    let mut names = vec![program.to_owned()];
    if !has_extension {
        names.extend(
            extensions
                .iter()
                .map(|extension| format!("{program}{extension}")),
        );
    }
    std::env::split_paths(search_path?)
        .filter(|dir| dir.is_absolute())
        .find_map(|dir| {
            names
                .iter()
                .map(|name| dir.join(name))
                .find(|candidate| candidate.is_file())
        })
}

/// The extensions a bare program name may leave out: `PATHEXT` on Windows, none elsewhere.
fn program_extensions() -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned())
        .split(';')
        .filter(|extension| extension.starts_with('.') && extension.len() > 1)
        .map(str::to_owned)
        .collect()
}

/// Starts `launch` and leaves it running.
///
/// # Errors
///
/// [`UserCommandError::RunAsAdminUnsupported`] for a start as administrator outside Windows
/// (nothing starts), [`UserCommandError::ProgramNotFound`] for a program not on the search path,
/// [`UserCommandError::UnsafeArgument`] for a batch file with a value cmd would read as syntax,
/// or the start failed.
pub(crate) async fn start(launch: Launch) -> Result<Started, EngineError> {
    if launch.elevated && !cfg!(windows) {
        return Err(UserCommandError::RunAsAdminUnsupported.into());
    }
    spawn(launch).await
}

/// Whether `launch` goes through the shell: as administrator, or minimized or maximized, which
/// `std::process::Command` cannot ask for.
fn through_shell(launch: &Launch) -> bool {
    launch.elevated || (cfg!(windows) && launch.show != Show::Normal)
}

#[cfg(not(test))]
async fn spawn(launch: Launch) -> Result<Started, EngineError> {
    let program = resolve(&launch)?;
    if through_shell(&launch) {
        return start_through_shell(launch, program).await;
    }
    let mut child = tokio::process::Command::from(direct_command(&launch, &program))
        .spawn()
        .map_err(|source| UserCommandError::Failed { source })?;
    tokio::spawn(async move {
        // How the program ended is its own window's to show.
        let _ = child.wait().await;
    });
    Ok(Started::Started)
}

/// The tests start nothing: each launch is recorded in [`STARTED`] after the checks a start
/// makes; a program the tests name but no folder holds is recorded too, unless it goes through
/// the shell, which needs it found.
#[cfg(test)]
async fn spawn(launch: Launch) -> Result<Started, EngineError> {
    use crate::recover::LockOrRecover as _;
    match resolve(&launch) {
        Ok(_) => {}
        Err(UserCommandError::ProgramNotFound) if !through_shell(&launch) => {}
        Err(error) => return Err(error.into()),
    }
    STARTED
        .lock_or_recover("user_commands.started")
        .push(launch);
    Ok(Started::Started)
}

/// What the tests started, in order.
#[cfg(test)]
pub(crate) static STARTED: std::sync::Mutex<Vec<Launch>> = std::sync::Mutex::new(Vec::new());

/// A start through the shell: the program found on the search path first.
#[cfg(not(test))]
async fn start_through_shell(launch: Launch, program: PathBuf) -> Result<Started, EngineError> {
    use crate::commands::elevate::{ShellStart, ShellStarted, shell_start};

    let started = shell_start(ShellStart {
        program,
        params: launch.args.join(" "),
        folder: launch.folder,
        show: launch.show,
        elevated: launch.elevated,
    })
    .await?;
    Ok(match started {
        ShellStarted::Started => Started::Started,
        ShellStarted::Cancelled => Started::Cancelled,
    })
}

/// `launch` as a command that starts `program` (as found and checked) without a shell, its
/// parameters given as this system's programs read them.
#[cfg(not(test))]
fn direct_command(launch: &Launch, program: &Path) -> std::process::Command {
    // console-window: allow (an own command's program shows its window, as in Total Commander)
    let mut command = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        for text in &launch.args {
            command.raw_arg(text);
        }
    }
    #[cfg(not(windows))]
    command.args(&launch.args);
    if let Some(folder) = &launch.folder {
        command.current_dir(folder);
    }
    command
}

#[cfg(test)]
#[path = "launch_tests.rs"]
mod tests;
