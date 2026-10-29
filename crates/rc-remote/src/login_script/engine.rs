//! [`run_script`]: a login script played against an async byte stream.
//!
//! The stream is anything that reads and writes bytes ([`ScriptStream`]; every tokio
//! `AsyncRead + AsyncWrite` is one), so the tests drive it with an in-memory pipe and RC-060-03
//! with the SSH PTY channel. Output goes into a buffer of at most [`BUFFER_LIMIT`] bytes, without
//! escape sequences; a `wait_for` matches against what arrived since the last match, then drops
//! it up to the match's end, so a prompt split over two blocks is found and the next step never
//! matches the same prompt again.
//!
//! Every secret the steps send is resolved before the first byte is written: a locked vault
//! answers [`ScriptError::VaultLocked`] while nothing has been sent. A secret's value never
//! reaches a log line, an event or an error; [`ScriptEvent::SecretSent`] marks only *when* one
//! was sent, so the caller can mask the echo. The run ends at the step's or the script's timeout,
//! on cancellation, or when the stream closes, each with the step it was at.

use std::{io, time::Duration};

use async_trait::async_trait;
use secrecy::{ExposeSecret as _, SecretString};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

use super::{LoginScript, Pattern, ScriptInvalid, SecretRef, Step, ansi::AnsiStripper};

/// Bytes of output kept for matching; older output is dropped first.
pub const BUFFER_LIMIT: usize = 64 * 1024;

/// What Enter sends on a terminal.
pub const ENTER: &[u8] = b"\r";

/// Bytes read at once.
const READ_CHUNK: usize = 4096;

/// A byte stream a script runs on: a terminal's input and output.
#[async_trait]
pub trait ScriptStream: Send {
    /// Reads what has arrived into `buf`, waiting for at least one byte; 0 when the stream
    /// ended. The engine puts its own timeout around it.
    async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;

    /// Writes all of `bytes`, flushed.
    async fn write_all(&mut self, bytes: &[u8]) -> io::Result<()>;
}

#[async_trait]
impl<T: AsyncRead + AsyncWrite + Unpin + Send> ScriptStream for T {
    async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        tokio::io::AsyncReadExt::read(self, buf).await
    }

    async fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        tokio::io::AsyncWriteExt::write_all(self, bytes).await?;
        tokio::io::AsyncWriteExt::flush(self).await
    }
}

/// Where the secrets of `send_secret` steps come from: the vault, through the engine.
#[async_trait]
pub trait SecretSource: Sync {
    /// The value `source` refers to.
    async fn secret(&self, source: SecretRef) -> Result<SecretString, SecretError>;
}

/// Why a secret cannot be had.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretError {
    /// The vault is locked; only the person can unlock it.
    Locked,
    /// Nothing is stored there.
    Missing,
    /// Another failure, by its stable code (`vault_undecryptable`, `database_error`, ...).
    Failed { code: &'static str },
}

/// How a run ended. A step is its index in the script, from 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ScriptOutcome {
    /// Every step ran.
    Completed,
    /// The step's or the whole script's timeout ran out at `step`.
    TimedOut { step: usize },
    /// The person cancelled.
    Cancelled,
    /// The stream ended or failed at `step`.
    StreamClosed { step: usize },
}

/// What a run reports as it goes; never a sent value or the output itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ScriptEvent {
    /// `step` begins.
    Step { step: usize },
    /// The pattern of `step` matched; `end` is where the match ends, in bytes of the output
    /// since the run began, escape sequences removed.
    Matched { step: usize, end: usize },
    /// The text of `step` was sent.
    Sent { step: usize },
    /// The secret of `step` was sent: the output that follows may echo it.
    SecretSent { step: usize },
}

/// Why a run did not start. Nothing was sent.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ScriptError {
    /// A step sends a stored secret and the vault is locked.
    #[error("the vault is locked; a login script's secret needs it unlocked")]
    VaultLocked,
    /// The secret `step` sends is not stored.
    #[error("step {step} sends a secret that is not stored")]
    SecretMissing { step: usize },
    /// A secret could not be read, by its code.
    #[error("a login script's secret cannot be read ({code})")]
    SecretFailed { code: &'static str },
    /// The script breaks a bound.
    #[error(transparent)]
    Invalid(#[from] ScriptInvalid),
}

impl ScriptError {
    /// The codes of this type's own variants; `SecretFailed` keeps its source's, `Invalid` one
    /// of [`ScriptInvalid::ALL_CODES`].
    pub const ALL_CODES: &[&str] = &["vault_locked", "login_script_secret_missing"];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::VaultLocked => "vault_locked",
            Self::SecretMissing { .. } => "login_script_secret_missing",
            Self::SecretFailed { code } => code,
            Self::Invalid(invalid) => invalid.code(),
        }
    }
}

/// [`run_script_observed`] without watching.
///
/// # Errors
///
/// As [`run_script_observed`].
pub async fn run_script(
    script: &LoginScript,
    stream: &mut dyn ScriptStream,
    secrets: &dyn SecretSource,
    cancel: &CancellationToken,
) -> Result<ScriptOutcome, ScriptError> {
    run_script_observed(script, stream, secrets, cancel, &mut |_| {}).await
}

/// Runs `script` on `stream`, telling `observe` each step, match and send.
///
/// # Errors
///
/// Before anything is sent: the script is invalid, or a secret it sends cannot be had
/// ([`ScriptError::VaultLocked`] for a locked vault). Everything after that is an outcome.
pub async fn run_script_observed(
    script: &LoginScript,
    stream: &mut dyn ScriptStream,
    secrets: &dyn SecretSource,
    cancel: &CancellationToken,
    observe: &mut (dyn FnMut(ScriptEvent) + Send),
) -> Result<ScriptOutcome, ScriptError> {
    script.validate()?;
    let resolved = resolve(script, secrets).await?;
    let deadline = Instant::now() + Duration::from_millis(script.total_timeout_ms);
    let mut output = Output::default();
    for (step, action) in script.steps.iter().enumerate() {
        if cancel.is_cancelled() {
            return Ok(ScriptOutcome::Cancelled);
        }
        observe(ScriptEvent::Step { step });
        tracing::debug!(step, action = name(action), "login script step");
        let progress = match action {
            Step::WaitFor {
                pattern,
                timeout_ms,
            } => {
                let ms = timeout_ms.unwrap_or(script.step_timeout_ms);
                let until = deadline.min(Instant::now() + Duration::from_millis(ms));
                let progress = wait_for(stream, &mut output, pattern, until, cancel).await;
                if progress == Progress::Done {
                    observe(ScriptEvent::Matched {
                        step,
                        end: output.consumed,
                    });
                }
                progress
            }
            Step::SendText { text, newline } => {
                let progress = send(stream, text.as_bytes(), *newline, deadline, cancel).await;
                if progress == Progress::Done {
                    observe(ScriptEvent::Sent { step });
                }
                progress
            }
            Step::SendSecret { source, newline } => {
                let value = resolved
                    .iter()
                    .find(|(kept, _)| kept == source)
                    .map(|(_, value)| value.expose_secret().as_bytes())
                    .unwrap_or_default();
                let progress = send(stream, value, *newline, deadline, cancel).await;
                if progress == Progress::Done {
                    observe(ScriptEvent::SecretSent { step });
                }
                progress
            }
            Step::SendLine => {
                let progress = send(stream, b"", true, deadline, cancel).await;
                if progress == Progress::Done {
                    observe(ScriptEvent::Sent { step });
                }
                progress
            }
            Step::Sleep { ms } => pause(*ms, deadline, cancel).await,
        };
        match progress {
            Progress::Done => {}
            Progress::TimedOut => {
                tracing::debug!(step, "login script timed out");
                return Ok(ScriptOutcome::TimedOut { step });
            }
            Progress::Cancelled => return Ok(ScriptOutcome::Cancelled),
            Progress::Closed => return Ok(ScriptOutcome::StreamClosed { step }),
        }
    }
    Ok(ScriptOutcome::Completed)
}

/// How one step went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Progress {
    Done,
    TimedOut,
    Cancelled,
    Closed,
}

/// The output since the last match, without escape sequences.
#[derive(Default)]
pub(super) struct Output {
    stripper: AnsiStripper,
    text: String,
    /// Bytes of stripped output dropped so far: matched or pushed out by the limit.
    pub(super) consumed: usize,
}

impl Output {
    /// Adds a block of output, dropping the oldest text beyond [`BUFFER_LIMIT`].
    pub(super) fn push(&mut self, block: &[u8]) {
        self.text.push_str(&self.stripper.push(block));
        if self.text.len() > BUFFER_LIMIT {
            let mut cut = self.text.len() - BUFFER_LIMIT;
            while !self.text.is_char_boundary(cut) {
                cut += 1;
            }
            self.text.drain(..cut);
            self.consumed += cut;
        }
    }

    /// Whether `pattern` matches; the text up to the match's end is dropped when it does.
    pub(super) fn take_match(&mut self, pattern: &Pattern) -> bool {
        let Some(end) = pattern.match_end(&self.text) else {
            return false;
        };
        self.text.drain(..end);
        self.consumed += end;
        true
    }
}

/// Every secret the steps send, read once before the run.
async fn resolve(
    script: &LoginScript,
    secrets: &dyn SecretSource,
) -> Result<Vec<(SecretRef, SecretString)>, ScriptError> {
    let mut resolved = Vec::new();
    for source in script.secret_refs() {
        match secrets.secret(source).await {
            Ok(value) => resolved.push((source, value)),
            Err(SecretError::Locked) => return Err(ScriptError::VaultLocked),
            Err(SecretError::Missing) => {
                let step = script
                    .steps
                    .iter()
                    .position(
                        |step| matches!(step, Step::SendSecret { source: s, .. } if *s == source),
                    )
                    .unwrap_or_default();
                return Err(ScriptError::SecretMissing { step });
            }
            Err(SecretError::Failed { code }) => return Err(ScriptError::SecretFailed { code }),
        }
    }
    Ok(resolved)
}

async fn wait_for(
    stream: &mut dyn ScriptStream,
    output: &mut Output,
    pattern: &Pattern,
    until: Instant,
    cancel: &CancellationToken,
) -> Progress {
    let mut chunk = [0_u8; READ_CHUNK];
    loop {
        if output.take_match(pattern) {
            return Progress::Done;
        }
        let read = tokio::select! {
            biased;
            () = cancel.cancelled() => return Progress::Cancelled,
            read = tokio::time::timeout_at(until, stream.read(&mut chunk)) => read,
        };
        match read {
            Err(_elapsed) => return Progress::TimedOut,
            Ok(Ok(0)) => return Progress::Closed,
            Ok(Ok(read)) => output.push(&chunk[..read]),
            Ok(Err(error)) => {
                tracing::debug!(kind = ?error.kind(), "login script stream failed reading");
                return Progress::Closed;
            }
        }
    }
}

/// Writes `bytes`, then Enter when `newline`.
async fn send(
    stream: &mut dyn ScriptStream,
    bytes: &[u8],
    newline: bool,
    deadline: Instant,
    cancel: &CancellationToken,
) -> Progress {
    let write = async {
        if !bytes.is_empty() {
            stream.write_all(bytes).await?;
        }
        if newline {
            stream.write_all(ENTER).await?;
        }
        Ok::<_, io::Error>(())
    };
    tokio::select! {
        biased;
        () = cancel.cancelled() => Progress::Cancelled,
        written = tokio::time::timeout_at(deadline, write) => match written {
            Err(_elapsed) => Progress::TimedOut,
            Ok(Ok(())) => Progress::Done,
            Ok(Err(error)) => {
                tracing::debug!(kind = ?error.kind(), "login script stream failed writing");
                Progress::Closed
            }
        },
    }
}

/// Sleeps `ms`, or until the script's deadline (then it timed out) or a cancellation.
async fn pause(ms: u64, deadline: Instant, cancel: &CancellationToken) -> Progress {
    let wanted = Instant::now() + Duration::from_millis(ms);
    let until = wanted.min(deadline);
    tokio::select! {
        biased;
        () = cancel.cancelled() => Progress::Cancelled,
        () = tokio::time::sleep_until(until) => {
            if wanted > deadline { Progress::TimedOut } else { Progress::Done }
        }
    }
}

/// A step's kind for the log.
fn name(step: &Step) -> &'static str {
    match step {
        Step::WaitFor { .. } => "wait_for",
        Step::SendText { .. } => "send_text",
        Step::SendSecret { .. } => "send_secret",
        Step::SendLine => "send_line",
        Step::Sleep { .. } => "sleep",
    }
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
