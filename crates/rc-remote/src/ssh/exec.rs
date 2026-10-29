//! One command on a session channel (RC-060-04): `exec` without a terminal, standard output and
//! standard error collected apart, the exit code or signal, under a time and a size limit.
//!
//! The command goes to the server exactly as it is given: the user's login shell there runs it,
//! in the login environment, with no login script and no pseudo-terminal - nothing can answer a
//! password prompt (`sudo` with a password is the terminal's). Each stream keeps at most
//! [`ExecLimits::max_output`] bytes and says when it lost more; the rest is still read and
//! dropped, so a full window never stalls the command. Standard input, when given, is sent while
//! the output is read and then closed; without it the input is closed at once, so a command that
//! reads it ends instead of waiting.
//!
//! The time limit ends a run with what arrived so far and `timed_out`. A run is cancelled by
//! dropping its future. Either way the command is sent `TERM` and the channel is closed; the
//! session and its other channels stay.

use std::time::{Duration, Instant};

use russh::{ChannelMsg, ChannelReadHalf, ChannelWriteHalf, Sig, client};
use serde::{Deserialize, Serialize};

use crate::ssh::{SshChannel, SshError};

/// How long a command may run unless the caller says otherwise.
pub const DEFAULT_EXEC_TIMEOUT: Duration = Duration::from_secs(60);
/// The longest time limit a caller may set.
pub const MAX_EXEC_TIMEOUT: Duration = Duration::from_secs(3600);
/// How many bytes of each stream are kept unless the caller says otherwise: 1 MiB.
pub const DEFAULT_EXEC_OUTPUT: usize = 1024 * 1024;
/// The most bytes of each stream a caller may ask to keep: 4 MiB.
pub const MAX_EXEC_OUTPUT: usize = 4 * 1024 * 1024;

/// The bounds of one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecLimits {
    pub timeout: Duration,
    /// Bytes kept of standard output, and as many of standard error.
    pub max_output: usize,
}

impl Default for ExecLimits {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_EXEC_TIMEOUT,
            max_output: DEFAULT_EXEC_OUTPUT,
        }
    }
}

/// What a command left behind. The streams are text; bytes that are not UTF-8 read as U+FFFD.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecOutput {
    pub stdout: String,
    pub stderr: String,
    /// Standard output went past the limit; only its start is kept.
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    /// The command's exit status; `None` when it ended by a signal, ran out of time, or the
    /// server named none.
    pub exit_code: Option<u32>,
    /// The signal that ended it (`KILL`, `TERM`, ...).
    pub exit_signal: Option<String>,
    /// It ran past the time limit and was stopped.
    pub timed_out: bool,
    pub duration_ms: u64,
}

/// Runs `command` on `channel` (a new session channel) with `stdin` as its input, within
/// `limits`.
///
/// # Errors
///
/// The server refuses the command ([`SshError::Protocol`]), or the connection goes away before
/// the command ends ([`SshError::ConnectionLost`]).
pub async fn exec_command(
    channel: SshChannel,
    command: &str,
    stdin: Option<&[u8]>,
    limits: ExecLimits,
) -> Result<ExecOutput, SshError> {
    let started = Instant::now();
    let (mut read, write) = channel.split();
    let mut closer = Closer(Some(write));
    let mut collected = Collected::new(limits.max_output);
    let finished = {
        let write = closer.write()?;
        write
            .exec(true, command)
            .await
            .map_err(|error| SshError::transport(&error))?;
        let run = feed_and_collect(write, &mut read, stdin, &mut collected);
        tokio::select! {
            () = tokio::time::sleep(limits.timeout) => Ok(false),
            done = run => done.map(|()| true),
        }
    };
    match finished {
        // The server closed the channel: nothing is left to stop.
        Ok(true) => closer.forget(),
        Ok(false) | Err(_) => closer.stop().await,
    }
    let timed_out = !finished?;
    if !timed_out && !collected.closed && collected.exit_code.is_none() {
        return Err(SshError::ConnectionLost);
    }
    Ok(collected.into_output(timed_out, started.elapsed()))
}

/// Sends `stdin` and the end of the input while reading every message until the server closes
/// the channel.
async fn feed_and_collect(
    write: &ChannelWriteHalf<client::Msg>,
    read: &mut ChannelReadHalf,
    stdin: Option<&[u8]>,
    collected: &mut Collected,
) -> Result<(), SshError> {
    let feed = async {
        if let Some(input) = stdin {
            write.data(input).await?;
        }
        write.eof().await
    };
    let mut feed = std::pin::pin!(feed);
    let mut fed = false;
    loop {
        tokio::select! {
            // A command that stops reading its input early is no failure: what it left unread
            // is simply not sent.
            _ = &mut feed, if !fed => fed = true,
            message = read.wait() => {
                let Some(message) = message else {
                    return Ok(());
                };
                if !collected.take(message)? {
                    return Ok(());
                }
            }
        }
    }
}

/// The streams and the end as they arrive.
struct Collected {
    max: usize,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdout_truncated: bool,
    stderr_truncated: bool,
    exit_code: Option<u32>,
    exit_signal: Option<String>,
    accepted: bool,
    closed: bool,
}

impl Collected {
    fn new(max: usize) -> Self {
        Self {
            max,
            stdout: Vec::new(),
            stderr: Vec::new(),
            stdout_truncated: false,
            stderr_truncated: false,
            exit_code: None,
            exit_signal: None,
            accepted: false,
            closed: false,
        }
    }

    /// Takes one message; `false` once the channel is closed.
    fn take(&mut self, message: ChannelMsg) -> Result<bool, SshError> {
        match message {
            ChannelMsg::Data { data } => {
                keep(
                    &mut self.stdout,
                    &mut self.stdout_truncated,
                    &data,
                    self.max,
                );
            }
            // Extended data of type 1 is standard error (RFC 4254, 5.2); no other type exists.
            ChannelMsg::ExtendedData { data, ext: 1 } => {
                keep(
                    &mut self.stderr,
                    &mut self.stderr_truncated,
                    &data,
                    self.max,
                );
            }
            ChannelMsg::ExitStatus { exit_status } => self.exit_code = Some(exit_status),
            ChannelMsg::ExitSignal { signal_name, .. } => {
                self.exit_signal = Some(signal_text(&signal_name));
            }
            ChannelMsg::Success => self.accepted = true,
            ChannelMsg::Failure if !self.accepted => {
                return Err(SshError::Protocol {
                    reason: "the server refused the command".to_owned(),
                });
            }
            ChannelMsg::Close => {
                self.closed = true;
                return Ok(false);
            }
            _ => {}
        }
        Ok(true)
    }

    fn into_output(self, timed_out: bool, took: Duration) -> ExecOutput {
        ExecOutput {
            stdout: String::from_utf8_lossy(&self.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&self.stderr).into_owned(),
            stdout_truncated: self.stdout_truncated,
            stderr_truncated: self.stderr_truncated,
            exit_code: if timed_out { None } else { self.exit_code },
            exit_signal: if timed_out { None } else { self.exit_signal },
            timed_out,
            duration_ms: u64::try_from(took.as_millis()).unwrap_or(u64::MAX),
        }
    }
}

/// Appends what fits of `data` to `buffer`; marks `truncated` when something did not.
fn keep(buffer: &mut Vec<u8>, truncated: &mut bool, data: &[u8], max: usize) {
    let room = max.saturating_sub(buffer.len());
    if data.len() > room {
        *truncated = true;
    }
    buffer.extend_from_slice(&data[..data.len().min(room)]);
}

/// A signal's name without the `SIG` prefix, as RFC 4254 names it.
fn signal_text(signal: &Sig) -> String {
    match signal {
        Sig::ABRT => "ABRT",
        Sig::ALRM => "ALRM",
        Sig::FPE => "FPE",
        Sig::HUP => "HUP",
        Sig::ILL => "ILL",
        Sig::INT => "INT",
        Sig::KILL => "KILL",
        Sig::PIPE => "PIPE",
        Sig::QUIT => "QUIT",
        Sig::SEGV => "SEGV",
        Sig::TERM => "TERM",
        Sig::USR1 => "USR1",
        Sig::Custom(name) => name.as_str(),
    }
    .to_owned()
}

/// The writing half of a run's channel: stops the command and closes the channel when the run
/// ends early, even when its future is dropped.
struct Closer(Option<ChannelWriteHalf<client::Msg>>);

impl Closer {
    fn write(&self) -> Result<&ChannelWriteHalf<client::Msg>, SshError> {
        self.0.as_ref().ok_or(SshError::ConnectionLost)
    }

    /// The channel ended by itself.
    fn forget(&mut self) {
        self.0 = None;
    }

    /// `TERM` to the command, then the channel closed. Best effort: a server that ignores
    /// signals still ends the command's input and output with the channel.
    async fn stop(&mut self) {
        if let Some(write) = self.0.take() {
            stop(write).await;
        }
    }
}

impl Drop for Closer {
    fn drop(&mut self) {
        if let (Some(write), Ok(runtime)) = (self.0.take(), tokio::runtime::Handle::try_current()) {
            runtime.spawn(stop(write));
        }
    }
}

async fn stop(write: ChannelWriteHalf<client::Msg>) {
    // A channel the server closed meanwhile refuses both; nothing is left to stop then.
    let _ = write.signal(Sig::TERM).await;
    let _ = write.close().await;
}

#[cfg(test)]
#[path = "exec_tests.rs"]
mod tests;
