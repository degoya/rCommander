//! Running git for a folder's status (RC-100-97), hardened against the repository's own
//! configuration, which anyone who prepared the folder - an unpacked archive, a clone - wrote.
//!
//! - **The program** is `panels.git_program` (protected: a program an agent could name would be
//!   code execution), else the first `git` of the absolute entries of `PATH` - on Windows then
//!   `%ProgramFiles%\Git\cmd\git.exe` - looked up once per run of the app. Never the panel's
//!   folder, never a relative entry. In a WSL distribution its own `/usr/bin/git` through
//!   `wsl.exe -d <distro>`.
//! - **The version**: the hardening is checked from [`MIN_VERSION`] on; an older git gives no
//!   marks and a log line.
//! - **The hardening**: `--no-optional-locks` (no index written, no hook after it),
//!   `core.fsmonitor=false`, `core.untrackedCache=false`, and every filter driver the
//!   configuration names (`git config -z --get-regexp '^filter\.'`, which only reads) emptied -
//!   `clean`, `smudge` and `process` empty, `required` false - through `GIT_CONFIG_COUNT`, so a
//!   driver's name may hold any character. `safe.directory` is never bypassed: a repository of
//!   another owner shows nothing. `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE` and their kin are
//!   removed from the environment; `GIT_TERMINAL_PROMPT=0`, `LC_ALL=C`.
//! - **The limits**: [`TIME_LIMIT`] per call, then the process is killed; at most
//!   [`OUTPUT_LIMIT`] of output, else no marks.
//!
//! git runs in the repository's root with the folder as a literal pathspec, so its paths are
//! the root's whatever the configuration says.

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    time::Duration,
};

use rc_process::NoConsoleWindow;
use tokio::{io::AsyncReadExt, process::Command};

use super::{PorcelainError, Status, parse_porcelain};

/// How long one git call may take before it is killed.
pub(crate) const TIME_LIMIT: Duration = Duration::from_secs(5);
/// The most output one git call may write; more gives no marks.
pub(crate) const OUTPUT_LIMIT: usize = 16 * 1024 * 1024;
/// The oldest git the hardening is checked for: 2.36.0, the first that reads `core.fsmonitor`
/// as a switch - before it, `false` named a hook program to run.
pub const MIN_VERSION: Version = Version(2, 36, 0);

/// The program's name in a folder of `PATH`.
const PROGRAM: &str = if cfg!(windows) { "git.exe" } else { "git" };
/// A WSL distribution's git.
const WSL_GIT: &str = "/usr/bin/git";
/// What would point git at another repository, index or configuration than the folder's.
const REMOVED: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG_PARAMETERS",
];

/// A git version, `major.minor.patch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u32, pub u32, pub u32);

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// The version `git --version` printed: `git version 2.45.1` or `git version 2.45.1.windows.1`.
pub(crate) fn parse_version(text: &str) -> Option<Version> {
    let number = text.trim().strip_prefix("git version ")?;
    let mut parts = number.split('.').map(|part| {
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        digits.parse::<u32>().ok()
    });
    let major = parts.next()??;
    let minor = parts.next()??;
    let patch = parts.next().flatten().unwrap_or(0);
    Some(Version(major, minor, patch))
}

/// The first `git` of `path_var`'s absolute folders, then (Windows) Git for Windows' own place
/// under `program_files`.
pub(crate) fn search(path_var: Option<&OsStr>, program_files: Option<&OsStr>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = path_var
        .map(|path| {
            std::env::split_paths(path)
                .filter(|folder| folder.is_absolute())
                .map(|folder| folder.join(PROGRAM))
                .collect()
        })
        .unwrap_or_default();
    if cfg!(windows)
        && let Some(root) = program_files
            .map(Path::new)
            .filter(|root| root.is_absolute())
    {
        candidates.push(root.join("Git").join("cmd").join("git.exe"));
    }
    candidates.into_iter().find(|candidate| candidate.is_file())
}

/// This process's `git`, by [`search`].
pub(crate) fn search_system() -> Option<PathBuf> {
    search(
        std::env::var_os("PATH").as_deref(),
        std::env::var_os("ProgramFiles").as_deref(),
    )
}

/// Where git runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Host {
    /// A program of this machine.
    Native(PathBuf),
    /// A WSL distribution's `/usr/bin/git`, through `wsl` (`wsl.exe` of the system folder).
    Wsl { wsl: PathBuf, distro: String },
}

impl Host {
    /// What names it in the log and in the version cache.
    pub(crate) fn key(&self) -> String {
        match self {
            Self::Native(program) => program.display().to_string(),
            Self::Wsl { distro, .. } => format!("wsl:{distro}"),
        }
    }

    /// The process of `call` on this host, not started yet.
    pub(crate) fn command(&self, call: &Call) -> Command {
        match self {
            Self::Native(program) => {
                let mut command = Command::new(program);
                command.args(&call.args).no_console_window();
                for name in REMOVED {
                    command.env_remove(name);
                }
                command.envs(call.env.iter().cloned());
                command
            }
            Self::Wsl { wsl, distro } => {
                let mut command = Command::new(wsl);
                let cd = if call.dir.is_empty() {
                    "/"
                } else {
                    call.dir.as_str()
                };
                command
                    .args(["-d", distro.as_str(), "--cd", cd, "-e", WSL_GIT])
                    .args(&call.args)
                    .no_console_window();
                command.envs(call.env.iter().cloned());
                // Only the variables WSLENV names cross into the distribution, unchanged.
                let names: Vec<String> = call
                    .env
                    .iter()
                    .map(|(name, _)| format!("{name}/u"))
                    .collect();
                command.env("WSLENV", names.join(":"));
                command
            }
        }
    }
}

/// One git call: its folder as the host sees it, its arguments and its environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Call {
    pub dir: String,
    pub args: Vec<OsString>,
    pub env: Vec<(String, String)>,
}

/// The environment every call gets, with `filters` emptied through `GIT_CONFIG_COUNT`.
fn environment(filters: &[String]) -> Vec<(String, String)> {
    let mut env = vec![
        ("GIT_TERMINAL_PROMPT".to_owned(), "0".to_owned()),
        ("LC_ALL".to_owned(), "C".to_owned()),
        ("GIT_OPTIONAL_LOCKS".to_owned(), "0".to_owned()),
        ("GIT_LITERAL_PATHSPECS".to_owned(), "1".to_owned()),
    ];
    let mut pairs = Vec::new();
    for name in filters {
        for (variable, value) in [
            ("clean", ""),
            ("smudge", ""),
            ("process", ""),
            ("required", "false"),
        ] {
            pairs.push((format!("filter.{name}.{variable}"), value.to_owned()));
        }
    }
    env.push(("GIT_CONFIG_COUNT".to_owned(), pairs.len().to_string()));
    for (index, (key, value)) in pairs.into_iter().enumerate() {
        env.push((format!("GIT_CONFIG_KEY_{index}"), key));
        env.push((format!("GIT_CONFIG_VALUE_{index}"), value));
    }
    env
}

/// `git --version`.
pub(crate) fn version_call() -> Call {
    Call {
        dir: String::new(),
        args: vec!["--version".into()],
        env: environment(&[]),
    }
}

/// The filter drivers the configuration of the repository in `dir` names; this only reads.
pub(crate) fn filters_call(dir: &str) -> Call {
    let args = [
        "--no-optional-locks",
        "-C",
        dir,
        "config",
        "-z",
        "--get-regexp",
        r"^filter\.",
    ];
    Call {
        dir: dir.to_owned(),
        args: args.into_iter().map(OsString::from).collect(),
        env: environment(&[]),
    }
}

/// The status of `pathspec` (a folder relative to the root, `.` for the root) in the
/// repository rooted at `dir`, with every one of `filters` emptied; ignored entries with
/// `ignored`.
pub(crate) fn status_call(dir: &str, pathspec: &str, filters: &[String], ignored: bool) -> Call {
    let mut args: Vec<OsString> = [
        "--no-optional-locks",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.untrackedCache=false",
        "-C",
        dir,
        "status",
        "--porcelain=v2",
        "-z",
        "--branch",
        "--untracked-files=normal",
        "--ignore-submodules=all",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    if ignored {
        args.push("--ignored=traditional".into());
    }
    args.push("--".into());
    args.push(if pathspec.is_empty() { "." } else { pathspec }.into());
    Call {
        dir: dir.to_owned(),
        args,
        env: environment(filters),
    }
}

/// The filter drivers' names in the answer of [`filters_call`]: `filter.<name>.<variable>`,
/// a line feed, the value, a NUL - each name once.
pub(crate) fn filter_names(bytes: &[u8]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for entry in bytes.split(|byte| *byte == 0) {
        let key = entry
            .split(|byte| *byte == b'\n')
            .next()
            .unwrap_or_default();
        let key = String::from_utf8_lossy(key);
        let Some((name, _variable)) = key
            .strip_prefix("filter.")
            .and_then(|rest| rest.rsplit_once('.'))
        else {
            continue;
        };
        if !name.is_empty() && !names.iter().any(|known| known == name) {
            names.push(name.to_owned());
        }
    }
    names
}

/// Why a git call gave nothing.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RunError {
    #[error("cannot be started: {0}")]
    Start(std::io::Error),
    #[error("no answer within {0:?}, killed")]
    TimedOut(Duration),
    #[error("more than {0} bytes of output, killed")]
    TooLong(usize),
    #[error("reading its answer failed: {0}")]
    Read(std::io::Error),
}

/// What `command` wrote and how it ended: killed after `limit`, or once it wrote more than
/// `max` bytes. No console window, no input, its errors not read.
pub(crate) async fn capture(
    mut command: Command,
    limit: Duration,
    max: usize,
) -> Result<(ExitStatus, Vec<u8>), RunError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .no_console_window();
    let mut child = command.spawn().map_err(RunError::Start)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| RunError::Read(std::io::Error::other("no output pipe")))?;
    let run = async {
        let mut bytes = Vec::new();
        let read = u64::try_from(max).unwrap_or(u64::MAX).saturating_add(1);
        stdout
            .take(read)
            .read_to_end(&mut bytes)
            .await
            .map_err(RunError::Read)?;
        if bytes.len() > max {
            return Err(RunError::TooLong(max));
        }
        let status = child.wait().await.map_err(RunError::Read)?;
        Ok((status, bytes))
    };
    // Returning drops the child: a git still running is killed (`kill_on_drop`).
    tokio::time::timeout(limit, run)
        .await
        .map_err(|_| RunError::TimedOut(limit))?
}

/// Why a folder got no marks from git.
#[derive(Debug, thiserror::Error)]
pub(crate) enum GitError {
    #[error(transparent)]
    Run(#[from] RunError),
    /// It ended with an error: no repository after all, one of another owner, a broken one.
    #[error("ended with {0}")]
    Failed(ExitStatus),
    #[error("answered in a form not understood: {0}")]
    Porcelain(#[from] PorcelainError),
}

/// The version of `host`'s git; `None` when it names none.
pub(crate) async fn version(host: &Host) -> Result<Option<Version>, GitError> {
    let (ended, bytes) = capture(host.command(&version_call()), TIME_LIMIT, 4096).await?;
    if !ended.success() {
        return Err(GitError::Failed(ended));
    }
    Ok(parse_version(&String::from_utf8_lossy(&bytes)))
}

/// The status of `pathspec` in the repository rooted at `dir` on `host`, each call within
/// `limit`: the filter drivers the configuration names are read first, then emptied for the
/// status itself.
pub(crate) async fn status(
    host: &Host,
    dir: &str,
    pathspec: &str,
    ignored: bool,
    limit: Duration,
) -> Result<Status, GitError> {
    let (ended, bytes) = capture(host.command(&filters_call(dir)), limit, OUTPUT_LIMIT).await?;
    // 1: the configuration names no filter.
    let filters = match ended.code() {
        Some(0) => filter_names(&bytes),
        Some(1) => Vec::new(),
        _ => return Err(GitError::Failed(ended)),
    };
    let call = status_call(dir, pathspec, &filters, ignored);
    let (ended, bytes) = capture(host.command(&call), limit, OUTPUT_LIMIT).await?;
    if !ended.success() {
        return Err(GitError::Failed(ended));
    }
    Ok(parse_porcelain(&bytes)?)
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
