//! `ssh.exec` (RC-060-04): one command on a channel of the connection's pooled session, its
//! streams and its end ([`rc_remote::ssh::exec_command`]).
//!
//! The command is the caller's intent and goes to the server unchanged; rCommander never builds
//! one from parts. It is checked for bounds only (not empty, at most [`MAX_EXEC_COMMAND`] bytes,
//! no NUL, which the server would cut it at) and logged masked: credentials by `rc_db::redact`,
//! then every secret the vault gave out. Through MCP the class `exec` makes the gate ask first;
//! the answer leaves through the MCP mask, which replaces the vault's secrets in the output too.
//! Dropping the call stops the command and closes its channel.

use std::time::Duration;

use rc_remote::ssh::{
    DEFAULT_EXEC_OUTPUT, DEFAULT_EXEC_TIMEOUT, ExecLimits, MAX_EXEC_OUTPUT, MAX_EXEC_TIMEOUT,
    exec_command,
};

use crate::{Engine, EngineError, Outcome, SessionChannel, SshExecRequest};

/// The longest command, in bytes.
pub const MAX_EXEC_COMMAND: usize = 8 * 1024;
/// The most standard input, in bytes.
pub const MAX_EXEC_STDIN: usize = 1024 * 1024;
/// How much of the command the log shows, in characters.
const LOGGED_CHARS: usize = 200;

impl Engine {
    /// `ssh.exec`.
    pub(crate) async fn ssh_exec(&self, request: &SshExecRequest) -> Result<Outcome, EngineError> {
        let limits = limits(request)?;
        let SessionChannel { channel, lease } = self.ssh_channel(request.connection_id).await?;
        tracing::info!(
            connection = %request.connection_id,
            command = %self.logged_command(&request.command),
            "SSH command"
        );
        let stdin = request.stdin.as_deref().map(str::as_bytes);
        let output = exec_command(channel, &request.command, stdin, limits).await;
        drop(lease);
        Ok(Outcome::SshExec(output?))
    }

    /// The command as the log may show it: masked and clipped.
    fn logged_command(&self, command: &str) -> String {
        let masked = rc_db::redact::redact_text(command);
        let masked = self.scrub_secrets(&masked).unwrap_or(masked);
        let mut shown: String = masked.chars().take(LOGGED_CHARS).collect();
        if shown.len() < masked.len() {
            shown.push_str("...");
        }
        shown
    }
}

/// The request's bounds checked, as the run's limits.
fn limits(request: &SshExecRequest) -> Result<ExecLimits, EngineError> {
    let invalid = |reason: &str| EngineError::ExecInvalid {
        reason: reason.to_owned(),
    };
    let command = &request.command;
    if command.trim().is_empty() {
        return Err(invalid("the command is empty"));
    }
    if command.len() > MAX_EXEC_COMMAND {
        return Err(invalid("the command is longer than 8 KiB"));
    }
    if command.contains('\0') {
        return Err(invalid("the command contains a NUL character"));
    }
    if request
        .stdin
        .as_ref()
        .is_some_and(|stdin| stdin.len() > MAX_EXEC_STDIN)
    {
        return Err(invalid("stdin is longer than 1 MiB"));
    }
    let timeout = match request.timeout_secs {
        None => DEFAULT_EXEC_TIMEOUT,
        Some(secs) if (1..=MAX_EXEC_TIMEOUT.as_secs()).contains(&secs) => Duration::from_secs(secs),
        Some(_) => return Err(invalid("timeout_secs is 1 to 3600")),
    };
    let max_output = match request.max_output_bytes {
        None => DEFAULT_EXEC_OUTPUT,
        Some(bytes) if (1..=MAX_EXEC_OUTPUT).contains(&bytes) => bytes,
        Some(_) => return Err(invalid("max_output_bytes is 1 to 4194304")),
    };
    Ok(ExecLimits {
        timeout,
        max_output,
    })
}
