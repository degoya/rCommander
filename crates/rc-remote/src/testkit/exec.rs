//! The test server's `exec`: a few shell forms acted out, enough for the command tests.
//!
//! `sh -c '<steps>'` runs its `;`-separated steps in order: `echo <text>` (with `>&2` to
//! standard error), `yes | head -c <n>` (`n` bytes of `y\n`, also with `>&2`), `sleep <seconds>`,
//! `exit <code>` and `kill -<SIGNAL> $$`, which end it. `cat` alone sends its input back until
//! the input ends. Any other command answers `ran: <command>` and exit status 0. Each run is a
//! task of its own, so a sleeping command holds up no other channel; a channel the client closes
//! stops its run, and the server counts the runs stopped so and records the signals it is sent.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Mutex, MutexGuard, PoisonError,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use russh::{
    ChannelId, Sig,
    server::{Handle, Session},
};
use tokio::task::AbortHandle;

/// The size of one data message the server sends.
const CHUNK: usize = 32 * 1024;

/// What the exec channels did, shared by every connection of one server.
#[derive(Default)]
pub struct ExecRuns {
    running: Mutex<HashMap<ChannelId, AbortHandle>>,
    /// The `cat` channels, which send their input back.
    echoing: Mutex<HashSet<ChannelId>>,
    /// Every exec channel; input to any other than a `cat` is dropped.
    exec: Mutex<HashSet<ChannelId>>,
    stopped: AtomicUsize,
    signals: Mutex<Vec<String>>,
}

impl ExecRuns {
    /// How many runs ended because the client closed their channel while they went on.
    #[must_use]
    pub fn stopped(&self) -> usize {
        self.stopped.load(Ordering::SeqCst)
    }

    /// The signals clients sent, by name.
    #[must_use]
    pub fn signals(&self) -> Vec<String> {
        lock(&self.signals).clone()
    }

    /// Starts `command` on `channel`.
    pub(super) fn start(
        &self,
        channel: ChannelId,
        command: &[u8],
        session: &mut Session,
    ) -> Result<(), russh::Error> {
        session.channel_success(channel)?;
        lock(&self.exec).insert(channel);
        let command = String::from_utf8_lossy(command).into_owned();
        if command == "cat" {
            lock(&self.echoing).insert(channel);
            return Ok(());
        }
        let steps = script(&command);
        let task = tokio::spawn(act(session.handle(), channel, steps));
        lock(&self.running).insert(channel, task.abort_handle());
        Ok(())
    }

    /// Whether input on `channel` is dropped rather than sent back.
    pub(super) fn swallows(&self, channel: ChannelId) -> bool {
        lock(&self.exec).contains(&channel) && !lock(&self.echoing).contains(&channel)
    }

    /// The client's input ended: a `cat` ends with it.
    pub(super) fn eof(
        &self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), russh::Error> {
        if !lock(&self.echoing).remove(&channel) {
            return Ok(());
        }
        session.exit_status_request(channel, 0)?;
        session.eof(channel)?;
        session.close(channel)
    }

    /// The client closed `channel`: a run still going on it stops.
    pub(super) fn closed(&self, channel: ChannelId) {
        let running = lock(&self.running).remove(&channel);
        let echoing = lock(&self.echoing).remove(&channel);
        if echoing || running.as_ref().is_some_and(|task| !task.is_finished()) {
            self.stopped.fetch_add(1, Ordering::SeqCst);
        }
        if let Some(task) = running {
            task.abort();
        }
        lock(&self.exec).remove(&channel);
    }

    pub(super) fn signal(&self, signal: &Sig) {
        lock(&self.signals).push(format!("{signal:?}"));
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One step of a command.
#[derive(Debug, PartialEq)]
enum Step {
    Out(Vec<u8>),
    Err(Vec<u8>),
    Sleep(Duration),
    Exit(u32),
    Killed(String),
}

/// The steps `command` stands for.
fn script(command: &str) -> Vec<Step> {
    let Some(body) = command
        .strip_prefix("sh -c '")
        .and_then(|rest| rest.strip_suffix('\''))
    else {
        let mut answer = b"ran: ".to_vec();
        answer.extend_from_slice(command.as_bytes());
        return vec![Step::Out(answer)];
    };
    body.split(';').filter_map(step).collect()
}

fn step(text: &str) -> Option<Step> {
    let text = text.trim();
    let (text, to_stderr) = match text.strip_suffix(">&2") {
        Some(rest) => (rest.trim_end(), true),
        None => (text, false),
    };
    let output = |bytes: Vec<u8>| {
        if to_stderr {
            Step::Err(bytes)
        } else {
            Step::Out(bytes)
        }
    };
    if let Some(words) = text.strip_prefix("echo ") {
        return Some(output(format!("{words}\n").into_bytes()));
    }
    if let Some(count) = text.strip_prefix("yes | head -c ") {
        let count: usize = count.trim().parse().ok()?;
        return Some(output(b"y\n".iter().copied().cycle().take(count).collect()));
    }
    if let Some(seconds) = text.strip_prefix("sleep ") {
        return Some(Step::Sleep(Duration::from_secs_f64(
            seconds.trim().parse().ok()?,
        )));
    }
    if let Some(code) = text.strip_prefix("exit ") {
        return Some(Step::Exit(code.trim().parse().ok()?));
    }
    let signal = text.strip_prefix("kill -")?.strip_suffix(" $$")?;
    Some(Step::Killed(signal.to_owned()))
}

/// Acts the steps out on `channel`; a send that fails means the client closed it.
async fn act(handle: Handle, channel: ChannelId, steps: Vec<Step>) {
    let mut status = Some(0);
    for step in steps {
        match step {
            Step::Out(bytes) => {
                for chunk in bytes.chunks(CHUNK) {
                    let _ = handle.data(channel, chunk.to_vec()).await;
                }
            }
            Step::Err(bytes) => {
                for chunk in bytes.chunks(CHUNK) {
                    let _ = handle.extended_data(channel, 1, chunk.to_vec()).await;
                }
            }
            Step::Sleep(time) => tokio::time::sleep(time).await,
            Step::Exit(code) => {
                status = Some(code);
                break;
            }
            Step::Killed(name) => {
                let _ = handle
                    .exit_signal_request(
                        channel,
                        Sig::Custom(name),
                        false,
                        String::new(),
                        String::new(),
                    )
                    .await;
                status = None;
                break;
            }
        }
    }
    if let Some(code) = status {
        let _ = handle.exit_status_request(channel, code).await;
    }
    let _ = handle.eof(channel).await;
    let _ = handle.close(channel).await;
}

#[cfg(test)]
#[path = "exec_tests.rs"]
mod tests;
