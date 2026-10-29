//! The login script of an SSH terminal (RC-060-03): the connection's script (RC-050-08) played
//! on the terminal's shell before the person types.
//!
//! The secrets the script sends are read before the terminal opens, in the command's own task:
//! there an MCP call opens no unlock dialog (`read_guard::from_mcp`), and a locked vault stops the
//! open with `vault_locked` before anything reaches the server. The script then runs in a task
//! of its own on a [`Tap`] of the shell's output, while the session holds typed input (Ctrl+C
//! cancels the script). Its progress and end are the terminal's [`LoginScriptRun`], announced as
//! [`TerminalChange::LoginScript`]; a failed run leaves the shell to the person as it is. Each
//! secret was remembered for the MCP mask as it was read, so an echo of one in the screen is
//! scrubbed; no log line, event or error carries a value.

use std::{
    io,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use async_trait::async_trait;
use rc_remote::login_script::{
    LoginScript, ScriptEvent, ScriptOutcome, ScriptStream, run_script_observed,
};
use rc_term::{Tap, TermId, TermSession};
use rc_vault::SecretString;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::{
    ConnectionId, Engine, EngineError, EngineEvent, LoginScriptId, LoginScriptInfo,
    ScriptSecretRef, SecretError, SecretSource, TerminalChange,
};

/// Ctrl+C, typed while the script runs: it cancels the script.
const CANCEL_KEY: u8 = 0x03;

/// A login script's run on a terminal, as `terminal.list` shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginScriptRun {
    pub script: LoginScriptId,
    pub name: String,
    /// How many steps the script has.
    pub steps: usize,
    #[serde(flatten)]
    pub state: LoginRunState,
}

/// Where a run stands. A step is its index in the script, from 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LoginRunState {
    /// It runs `step`; typed input waits until it ends.
    Running {
        step: usize,
    },
    Completed,
    /// The step's or the script's timeout ran out at `step`.
    TimedOut {
        step: usize,
    },
    /// The person cancelled it (Ctrl+C).
    Cancelled,
    /// The shell or the connection ended at `step`.
    StreamClosed {
        step: usize,
    },
    /// It did not start, by the stable code (`login_script_secret_missing`, ...).
    Failed {
        code: String,
    },
}

impl LoginRunState {
    /// Whether the run is over.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        !matches!(self, Self::Running { .. })
    }
}

/// A terminal's run, shared by the terminal's entry and the task that runs it.
#[derive(Clone, Default)]
pub(crate) struct LoginRunCell(Arc<Mutex<Option<LoginScriptRun>>>);

impl LoginRunCell {
    pub(crate) fn get(&self) -> Option<LoginScriptRun> {
        self.lock().clone()
    }

    fn set_state(&self, state: LoginRunState) {
        if let Some(run) = self.lock().as_mut() {
            run.state = state;
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<LoginScriptRun>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A script ready to run: checked, with every secret it sends read.
pub(crate) struct PreparedLogin {
    run: LoginScriptRun,
    script: LoginScript,
    secrets: Prefetched,
}

/// The secrets of one run, read before it: each a value, or why there is none.
struct Prefetched(Vec<(ScriptSecretRef, Result<SecretString, SecretError>)>);

#[async_trait]
impl SecretSource for Prefetched {
    async fn secret(&self, source: ScriptSecretRef) -> Result<SecretString, SecretError> {
        self.0
            .iter()
            .find(|(kept, _)| *kept == source)
            .map_or(Err(SecretError::Missing), |(_, value)| value.clone())
    }
}

impl Engine {
    /// The script `script` of `connection`, checked, its secrets read.
    ///
    /// # Errors
    ///
    /// The script is gone or breaks a bound; [`EngineError::VaultLocked`] when a secret it sends
    /// needs the locked vault (announced to the window for the connection's password, never for
    /// an MCP call). A secret that is not stored is the run's failure, not this one.
    pub(crate) async fn prepare_login(
        &self,
        script: LoginScriptId,
        connection: ConnectionId,
    ) -> Result<PreparedLogin, EngineError> {
        let info = LoginScriptInfo::from_record(self.db()?.login_script(script).await?)?;
        let model = info.script();
        model.validate()?;
        let source = self.login_script_secrets(script, Some(connection));
        let mut secrets = Vec::new();
        for reference in model.secret_refs() {
            let value = source.secret(reference).await;
            if matches!(value, Err(SecretError::Locked)) {
                return Err(EngineError::VaultLocked);
            }
            secrets.push((reference, value));
        }
        Ok(PreparedLogin {
            run: LoginScriptRun {
                script,
                name: info.name,
                steps: model.steps.len(),
                state: LoginRunState::Running { step: 0 },
            },
            script: model,
            secrets: Prefetched(secrets),
        })
    }

    /// Runs `login` on `session` through `tap`, typed input held until it ends; `then` is typed
    /// after a run that completed (the `cd` into the panel's folder).
    pub(crate) fn start_login(
        &self,
        session: &Arc<TermSession>,
        tap: Tap,
        login: PreparedLogin,
        cell: &LoginRunCell,
        then: Option<Vec<u8>>,
    ) {
        let cancel = CancellationToken::new();
        let typed = cancel.clone();
        session.hold_input(move |bytes| {
            if bytes.contains(&CANCEL_KEY) {
                typed.cancel();
            }
        });
        *cell.lock() = Some(login.run.clone());
        let run = LoginTask {
            session: Arc::clone(session),
            terminal: session.id(),
            cell: cell.clone(),
            events: self.inner.events.clone(),
        };
        run.announce();
        tokio::spawn(run.play(tap, login, cancel, then));
    }
}

/// What the task of one run holds: never the engine.
struct LoginTask {
    session: Arc<TermSession>,
    terminal: TermId,
    cell: LoginRunCell,
    events: broadcast::Sender<EngineEvent>,
}

impl LoginTask {
    fn announce(&self) {
        // No subscriber is not an error: a headless engine may have none.
        let _ = self.events.send(EngineEvent::Terminal {
            terminal: self.terminal,
            change: TerminalChange::LoginScript,
        });
    }

    async fn play(
        self,
        tap: Tap,
        login: PreparedLogin,
        cancel: CancellationToken,
        then: Option<Vec<u8>>,
    ) {
        let mut stream = TapStream {
            tap,
            pending: Vec::new(),
        };
        let mut observe = |event: ScriptEvent| {
            if let ScriptEvent::Step { step } = event {
                self.cell.set_state(LoginRunState::Running { step });
                self.announce();
            }
        };
        let outcome = run_script_observed(
            &login.script,
            &mut stream,
            &login.secrets,
            &cancel,
            &mut observe,
        )
        .await;
        let state = match outcome {
            Ok(ScriptOutcome::Completed) => LoginRunState::Completed,
            Ok(ScriptOutcome::TimedOut { step }) => LoginRunState::TimedOut { step },
            Ok(ScriptOutcome::Cancelled) => LoginRunState::Cancelled,
            Ok(ScriptOutcome::StreamClosed { step }) => LoginRunState::StreamClosed { step },
            Err(error) => LoginRunState::Failed {
                code: error.code().to_owned(),
            },
        };
        tracing::debug!(terminal = %self.terminal, state = ?state, "login script ended");
        // Only after a run that got through: a failed one may have left a prompt waiting for a
        // password, which must not be given a path.
        if state == LoginRunState::Completed
            && let Some(then) = then
        {
            let _ = stream.tap.write(then);
        }
        self.session.release_input();
        self.cell.set_state(state);
        self.announce();
    }
}

/// The tap as the script engine reads and writes it.
struct TapStream {
    tap: Tap,
    /// The rest of a chunk a read did not take.
    pending: Vec<u8>,
}

#[async_trait]
impl ScriptStream for TapStream {
    async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.pending.is_empty() {
            match self.tap.read().await {
                Some(chunk) => self.pending = chunk,
                None => return Ok(0),
            }
        }
        let taken = buf.len().min(self.pending.len());
        buf[..taken].copy_from_slice(&self.pending[..taken]);
        self.pending.drain(..taken);
        Ok(taken)
    }

    async fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.tap
            .write(bytes.to_vec())
            .map_err(|error| io::Error::new(io::ErrorKind::BrokenPipe, error.code()))
    }
}

#[cfg(test)]
#[path = "ssh_login_tests.rs"]
mod tests;
