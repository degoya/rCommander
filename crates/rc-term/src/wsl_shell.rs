//! The login shell of a WSL distribution, for the `cd` the app types (RC-080-65).
//!
//! A distribution's default shell need not be POSIX: with `tcsh` or `csh` a `!` is history even
//! inside single quotes. Before its first terminal the distribution is asked once, through
//! `wsl.exe -d <distro> -e sh -c 'getent passwd "$(id -un)"'` - a fixed line, the distribution
//! one of the detected names -, and the answer kept for the rest of the run. A distribution that
//! does not answer within [`QUERY_LIMIT`] is typed into as POSIX, as before, and asked again
//! next time. Outside Windows there is no WSL and nothing is started.

use crate::recover::LockOrRecover as _;
use std::{collections::BTreeMap, path::Path, process::Stdio, sync::Mutex, time::Duration};

use rc_process::NoConsoleWindow as _;

use crate::{ShellSyntax, wsl::plausible_name};

/// How long the first question to a distribution may take: it may have to boot.
const QUERY_LIMIT: Duration = Duration::from_secs(10);

/// What `sh` runs in the distribution: the user's `passwd` line.
const PASSWD_QUERY: &str = r#"getent passwd "$(id -un)""#;

/// The syntax per distribution (`None`: WSL's default) learned in this run.
static LEARNED: Mutex<BTreeMap<Option<String>, ShellSyntax>> = Mutex::new(BTreeMap::new());

/// The syntax of `distro`'s login shell (WSL's default distribution when `None`), asked of
/// `wsl.exe` at `program` once per run.
pub async fn wsl_login_syntax(program: &Path, distro: Option<&str>) -> ShellSyntax {
    let key = distro.map(str::to_owned);
    if let Some(known) = learned().get(&key) {
        return *known;
    }
    let Some(line) = ask(program, distro).await else {
        return ShellSyntax::Posix;
    };
    let syntax = syntax_of_passwd(&line);
    learned().insert(key, syntax);
    syntax
}

/// The syntax of the login shell a `passwd` line names: [`ShellSyntax::Csh`] for `csh` and
/// `tcsh`, [`ShellSyntax::Posix`] for every other (fish reads the POSIX line too).
#[must_use]
pub fn syntax_of_passwd(line: &str) -> ShellSyntax {
    let shell = line.trim().rsplit(':').next().unwrap_or_default();
    let name = shell.rsplit('/').next().unwrap_or_default();
    match name {
        "csh" | "tcsh" => ShellSyntax::Csh,
        _ => ShellSyntax::Posix,
    }
}

/// The arguments of the question to `distro`; `None` for a name `wsl.exe` could misread.
pub(crate) fn query_args(distro: Option<&str>) -> Option<Vec<String>> {
    let mut args = Vec::new();
    if let Some(distro) = distro {
        if !plausible_name(distro) {
            return None;
        }
        args.extend(["-d".to_owned(), distro.to_owned()]);
    }
    args.extend(["-e", "sh", "-c", PASSWD_QUERY].map(str::to_owned));
    Some(args)
}

/// The distribution's answer, when it gave one in time.
async fn ask(program: &Path, distro: Option<&str>) -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    let args = query_args(distro)?;
    let output = tokio::time::timeout(
        QUERY_LIMIT,
        tokio::process::Command::new(program)
            .no_console_window()
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await;
    match output {
        Ok(Ok(output)) if output.status.success() => {
            Some(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        _ => {
            tracing::debug!("a WSL distribution did not name its login shell");
            None
        }
    }
}

fn learned() -> std::sync::MutexGuard<'static, BTreeMap<Option<String>, ShellSyntax>> {
    LEARNED.lock_or_recover("wsl_shell.learned")
}

#[cfg(test)]
#[path = "wsl_shell_tests.rs"]
mod tests;
