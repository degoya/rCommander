//! Running `wsl.exe` under a time limit.
//!
//! Without WSL, or with WSL switched off, `wsl.exe` may print an installation prompt, fail, or
//! not answer at all. A drive bar must not wait for it, so every call has a limit, after which
//! the process is killed and nothing is listed; the log says which of the cases it was.

use std::{process::Stdio, time::Duration};

use rc_process::NoConsoleWindow;
use tokio::process::Command;

use super::decode;

/// What `command` wrote to standard output, or `None` when it could not start, ended with a
/// failure, or did not end within `limit` - it is killed then.
pub(crate) async fn capture(mut command: Command, limit: Duration) -> Option<Vec<u8>> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .no_console_window();
    let program = command
        .as_std()
        .get_program()
        .to_string_lossy()
        .into_owned();
    match tokio::time::timeout(limit, command.output()).await {
        Err(_) => {
            tracing::warn!(
                %program,
                ?limit,
                "no answer within the time limit, killed; no WSL distributions listed"
            );
            None
        }
        Ok(Err(error)) => {
            tracing::info!(%program, %error, "cannot be started; WSL is not installed");
            None
        }
        Ok(Ok(output)) if !output.status.success() => {
            tracing::info!(
                %program,
                status = %output.status,
                "failed; WSL is switched off or has no distribution"
            );
            None
        }
        Ok(Ok(output)) => Some(output.stdout),
    }
}

/// The distribution names `command` (`wsl.exe --list --quiet`) prints; none when it fails or
/// hangs.
pub(crate) async fn names_from(command: Command, limit: Duration) -> Vec<String> {
    capture(command, limit)
        .await
        .map(|bytes| decode::names(&decode::decode(&bytes)))
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "query_tests.rs"]
mod tests;
