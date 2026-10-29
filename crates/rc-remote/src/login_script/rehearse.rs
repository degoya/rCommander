//! [`rehearse`]: the editor's test run - a script played against pasted sample output.
//!
//! The engine runs as it would on a server, on a stream that reads the sample and swallows what
//! is sent. Secrets are not read: nothing reaches a server, so the vault stays shut and a
//! `send_secret` sends nothing. Sleeps are cut short. When the sample is used up the run ends
//! with `stream_closed` at the step that waited for more.

use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use async_trait::async_trait;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::sync::CancellationToken;

use super::{
    BUFFER_LIMIT, LoginScript, ScriptError, ScriptEvent, ScriptOutcome, SecretError, SecretRef,
    SecretSource, Step, run_script_observed, strip_ansi,
};

/// One step the test run reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RehearsalStep {
    pub step: usize,
    /// For a `wait_for` that matched: where the match ends in [`Rehearsal::text`], in bytes.
    pub end: Option<usize>,
}

/// What a test run showed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rehearsal {
    /// The sample as the patterns saw it: escape sequences removed, at most [`BUFFER_LIMIT`]
    /// bytes.
    pub text: String,
    /// The steps reached, in order.
    pub reached: Vec<RehearsalStep>,
    pub outcome: ScriptOutcome,
}

/// Plays `script` against `sample`.
///
/// # Errors
///
/// The script breaks a bound.
pub async fn rehearse(script: &LoginScript, sample: &str) -> Result<Rehearsal, ScriptError> {
    let mut cut = sample.len().min(BUFFER_LIMIT);
    while !sample.is_char_boundary(cut) {
        cut -= 1;
    }
    let sample = &sample.as_bytes()[..cut];
    let mut script = script.clone();
    for step in &mut script.steps {
        if let Step::Sleep { ms } = step {
            *ms = 1;
        }
    }
    let mut stream = Sample { rest: sample };
    let mut reached: Vec<RehearsalStep> = Vec::new();
    let outcome = run_script_observed(
        &script,
        &mut stream,
        &NoSecrets,
        &CancellationToken::new(),
        &mut |event| match event {
            ScriptEvent::Step { step } => reached.push(RehearsalStep { step, end: None }),
            ScriptEvent::Matched { end, .. } => {
                if let Some(last) = reached.last_mut() {
                    last.end = Some(end);
                }
            }
            ScriptEvent::Sent { .. } | ScriptEvent::SecretSent { .. } => {}
        },
    )
    .await?;
    Ok(Rehearsal {
        text: strip_ansi(sample),
        reached,
        outcome,
    })
}

/// Reads the sample once, then ends; swallows writes.
struct Sample<'a> {
    rest: &'a [u8],
}

impl AsyncRead for Sample<'_> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let rest: &[u8] = self.rest;
        let (now, later) = rest.split_at(rest.len().min(buf.remaining()));
        buf.put_slice(now);
        self.rest = later;
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for Sample<'_> {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// Answers every secret with nothing: a test run never opens the vault.
struct NoSecrets;

#[async_trait]
impl SecretSource for NoSecrets {
    async fn secret(&self, _: SecretRef) -> Result<SecretString, SecretError> {
        Ok(SecretString::from(""))
    }
}

#[cfg(test)]
#[path = "rehearse_tests.rs"]
mod tests;
