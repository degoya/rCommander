//! The `connections.connect`, `.disconnect`, `.test` and session-settings commands, the channel
//! the later SSH features open ([`Engine::ssh_channel`]), and the watch over each open session.

use std::{
    sync::{Arc, Weak, atomic::Ordering},
    time::{Duration, Instant},
};

use rc_remote::{
    SshError, SshSession,
    ssh::{SessionOptions, SshTarget},
};
use tokio_util::sync::CancellationToken;

use crate::{
    ConnectionId, ConnectionKind, Engine, EngineError, EngineEvent, Inner, Outcome,
    sessions::{
        ChannelLease, EngineCredentials, EngineGate, Live, SessionChange, SessionChannel,
        SessionPrefs, SessionStatus, SessionTestReport,
    },
    settings::{AGENT_MAX_KEYS_KEY, stored_setting},
};

/// The longest the watch over an idle session sleeps between two looks.
const IDLE_POLL: Duration = Duration::from_secs(5);

impl Engine {
    /// `connections.connect`: the connection's session, opened now unless it is up.
    pub(crate) async fn connect_session(
        &self,
        id: ConnectionId,
        announce: bool,
    ) -> Result<Outcome, EngineError> {
        self.quick_for_call(id)?;
        let live = self.session(id, announce).await?;
        Ok(Outcome::Session(live.status(id)))
    }

    /// `connections.disconnect`: closes the session and cancels a setup under way; for an FTP
    /// connection the session its panels and calls share (Ctrl+Shift+F on an `ftp://` panel):
    /// every control connection of it says `QUIT` and closes, and its end is announced as an SSH
    /// session's is ([`SessionChange::Closed`]). Every panel on the server leaves it for a local
    /// folder ([`Engine::leave_server`]); going there again logs in again. A quick connection is
    /// forgotten too, its secrets with it.
    pub(crate) async fn disconnect_session(
        &self,
        id: ConnectionId,
    ) -> Result<Outcome, EngineError> {
        let quick = self.quick_for_call(id)?.is_some();
        if !quick {
            self.db()?.connection(id).await?;
        }
        let (connecting, live) = self.inner.sessions.slot(id).close();
        if let Some(live) = &live {
            live.session.disconnect().await;
        }
        let ftp = self.forget_remote(id).await;
        if ftp {
            self.publish(EngineEvent::Session {
                connection: id,
                change: SessionChange::Closed,
            });
        }
        if quick {
            self.forget_quick(id);
            self.publish(EngineEvent::Drives);
        }
        self.leave_server(id).await;
        Ok(Outcome::SessionClosed {
            connection_id: id,
            was_open: connecting || live.is_some() || ftp,
        })
    }

    /// `connections.test`, one command for every connection kind: an FTP connection logs in once
    /// ([`Engine::test_ftp_connection`]), an SSH one runs [`Engine::test_ssh`].
    pub(crate) async fn test_session(
        &self,
        id: ConnectionId,
        announce: bool,
    ) -> Result<Outcome, EngineError> {
        let connection = self.db()?.connection(id).await?;
        if connection.kind.is_ftp() {
            return self.test_ftp_connection(id, announce).await;
        }
        Ok(Outcome::SessionTested(self.test_ssh(id, announce).await?))
    }

    /// Tests the SSH connection `id` with a session of its own, set up and closed again; the
    /// pool is not touched, and `connections.disconnect` cancels it like any setup. The body a
    /// `connections.test` shared by every connection kind calls for SSH; `announce` is off for
    /// MCP (a locked vault opens no dialog).
    pub(crate) async fn test_ssh(
        &self,
        id: ConnectionId,
        announce: bool,
    ) -> Result<SessionTestReport, EngineError> {
        let slot = self.inner.sessions.slot(id);
        let (cancel, _setup) = slot.begin_setup();
        let started = Instant::now();
        let session = self.open_session(id, announce, &cancel).await?;
        let report = SessionTestReport {
            connection_id: id,
            login: session.auth().clone(),
            host_key: session.host_key().clone(),
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        };
        session.disconnect().await;
        Ok(report)
    }

    /// `connections.session_prefs`.
    pub(crate) async fn get_session_prefs(&self, id: ConnectionId) -> Result<Outcome, EngineError> {
        self.db()?.connection(id).await?;
        Ok(Outcome::SessionPrefs {
            connection_id: id,
            prefs: self.session_prefs(id).await?,
        })
    }

    /// `connections.set_session_prefs`: used by the next session of the connection.
    pub(crate) async fn set_session_prefs(
        &self,
        id: ConnectionId,
        prefs: &SessionPrefs,
    ) -> Result<Outcome, EngineError> {
        let stored = prefs.checked()?;
        // A quick connection has no row: its settings are the defaults.
        self.db()?.connection(id).await?;
        self.db()?.set_session_prefs(id, &stored).await?;
        Ok(Outcome::SessionPrefs {
            connection_id: id,
            prefs: SessionPrefs::from_stored(stored),
        })
    }

    /// A new channel on the session of connection `id`, connecting first when it has none or
    /// lost it: what SFTP, the remote terminal and remote commands open. Keep the lease as long
    /// as the channel.
    ///
    /// # Errors
    ///
    /// The session cannot be opened (the codes of `connections.connect`), or the channel is
    /// refused.
    pub async fn ssh_channel(&self, id: ConnectionId) -> Result<SessionChannel, EngineError> {
        let live = self.session(id, true).await?;
        let lease = ChannelLease::new(Arc::clone(&live));
        match live.session.open_channel().await {
            Ok(channel) => Ok(SessionChannel { channel, lease }),
            // Lost between the look and the channel: once more, on a new session.
            Err(SshError::ConnectionLost) => {
                drop(lease);
                let live = self.session(id, true).await?;
                let lease = ChannelLease::new(Arc::clone(&live));
                let channel = live.session.open_channel().await?;
                Ok(SessionChannel { channel, lease })
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Every connection with a session up or being set up, by id.
    #[must_use]
    pub fn ssh_sessions(&self) -> Vec<SessionStatus> {
        self.inner.sessions.statuses()
    }

    /// The session of connection `id`, when one is up or being set up.
    #[must_use]
    pub fn ssh_session(&self, id: ConnectionId) -> Option<SessionStatus> {
        self.inner.sessions.status(id)
    }

    /// The connection's settings, the defaults where none are stored.
    async fn session_prefs(&self, id: ConnectionId) -> Result<SessionPrefs, EngineError> {
        Ok(self
            .db()?
            .session_prefs(id)
            .await?
            .map(SessionPrefs::from_stored)
            .unwrap_or_default())
    }

    /// The pooled session of `id`, opened when there is none or it is gone.
    pub(crate) async fn session(
        &self,
        id: ConnectionId,
        announce: bool,
    ) -> Result<Arc<Live>, EngineError> {
        // A quick connection's live session is the person's alone: never an MCP call's.
        self.quick_for_call(id)?;
        let slot = self.inner.sessions.slot(id);
        let _turn = slot.turn.lock().await;
        if let Some(live) = slot.live() {
            return Ok(live);
        }
        let (cancel, setup) = slot.begin_setup();
        let session = self.open_session(id, announce, &cancel).await?;
        if cancel.is_cancelled() {
            // Closed while the last step finished: it does not go into the pool.
            session.disconnect().await;
            return Err(EngineError::Cancelled);
        }
        let idle_close_secs = self.session_prefs(id).await?.idle_close_secs;
        let live = Arc::new(Live::new(session, idle_close_secs));
        slot.set_live(Arc::clone(&live));
        drop(setup);
        tokio::spawn(watch(Arc::downgrade(&self.inner), id, Arc::clone(&live)));
        self.publish(EngineEvent::Session {
            connection: id,
            change: SessionChange::Connected,
        });
        Ok(live)
    }

    /// A new session for the connection `id`, outside the pool.
    async fn open_session(
        &self,
        id: ConnectionId,
        announce: bool,
        cancel: &CancellationToken,
    ) -> Result<SshSession, EngineError> {
        if let Some(quick) = self.quick_for_call(id)? {
            return self.open_quick_session(quick, cancel).await;
        }
        let connection = self.login_connection(id).await?;
        if connection.kind != ConnectionKind::Ssh {
            return Err(EngineError::ConnectionNotSsh { id });
        }
        let prefs = self.session_prefs(id).await?;
        let target = SshTarget {
            host: connection.host,
            port: connection.port,
            user: connection.username.unwrap_or_default(),
        };
        let options = SessionOptions {
            keepalive: Duration::from_secs(u64::from(prefs.keepalive_secs)),
            auth_order: prefs.auth_order,
            agents: self.agent_sources().await?,
            agent_fingerprint: prefs.agent_fingerprint,
            own_key: connection.key.as_ref().map(|key| key.fingerprint.clone()),
            max_agent_keys: self.agent_max_keys().await,
            ..SessionOptions::default()
        };
        let gate = Arc::new(EngineGate {
            engine: self.clone(),
        });
        let credentials = EngineCredentials {
            engine: self.clone(),
            connection: id,
            key_path: connection.key_path,
            announce,
        };
        Ok(SshSession::connect(&target, &options, gate, &credentials, cancel).await?)
    }
}

impl Engine {
    /// `ssh.agent_max_keys`: the most agent keys one login offers (RC-080-43); its default
    /// without a database.
    pub(crate) async fn agent_max_keys(&self) -> u32 {
        let stored = match self.db() {
            Ok(db) => stored_setting(db, AGENT_MAX_KEYS_KEY).await,
            Err(_) => None,
        };
        stored
            .as_ref()
            .and_then(serde_json::Value::as_u64)
            .and_then(|limit| u32::try_from(limit).ok())
            .unwrap_or(rc_remote::ssh::DEFAULT_MAX_AGENT_KEYS)
    }
}

/// Watches one pooled session until it ends: closes it once idle for its idle time, and
/// announces its end - closed on purpose, or lost. Holds the engine only while announcing.
async fn watch(inner: Weak<Inner>, id: ConnectionId, live: Arc<Live>) {
    let poll = live.idle_close.map_or(IDLE_POLL, |idle| {
        (idle / 2).clamp(Duration::from_millis(100), IDLE_POLL)
    });
    loop {
        tokio::select! {
            () = live.session.closed() => break,
            () = tokio::time::sleep(poll) => {
                let idle = live.idle_for().zip(live.idle_close);
                if idle.is_some_and(|(idle, limit)| idle >= limit) {
                    live.closing.store(true, Ordering::SeqCst);
                    live.session.disconnect().await;
                }
            }
        }
    }
    let Some(inner) = inner.upgrade() else {
        return;
    };
    let engine = Engine { inner };
    engine.inner.sessions.slot(id).forget(&live);
    let change = if live.closing.load(Ordering::SeqCst) {
        SessionChange::Closed
    } else {
        tracing::warn!(connection = %id, "SSH connection lost");
        SessionChange::Lost
    };
    engine.publish(EngineEvent::Session {
        connection: id,
        change,
    });
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "sessions_pool_tests.rs"]
mod pool_tests;

#[cfg(test)]
#[path = "ssh_exec_tests.rs"]
mod exec_tests;
