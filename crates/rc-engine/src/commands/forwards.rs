//! The `forwards.*` commands (RC-110-03): a connection's local port forwards listed, saved,
//! started, stopped and deleted; their start with the session (`auto_start`) and their end with
//! it (`connections.disconnect`, a loss with `ssh.forward_reconnect` off).

use std::{
    net::SocketAddr,
    sync::{Arc, atomic::AtomicUsize},
};

use rc_remote::SshError;
use tokio_util::sync::CancellationToken;

use crate::{
    ConnectionId, Engine, EngineError, EngineEvent, Forward, ForwardDraft, Outcome,
    forwards::{
        ForwardChange, ForwardError, ForwardView, ForwardsOutcome, Running, is_loopback, listener,
    },
    sessions::{ChannelLease, SessionChannel},
    settings::{FORWARD_RECONNECT_KEY, FORWARDS_IN_STATUS_BAR_KEY, FORWARDS_LAN_KEY},
};

impl Engine {
    /// `forwards.list`: the forwards of `connection` in their order with their state, or every
    /// running one.
    pub(crate) async fn list_forwards(
        &self,
        connection: Option<ConnectionId>,
    ) -> Result<Outcome, EngineError> {
        let forwards = match connection {
            Some(id) => self.db()?.forwards(id).await?,
            None => self.running_forwards(),
        };
        let desk = &self.inner.sessions.forwards;
        let forwards = forwards
            .into_iter()
            .map(|forward| desk.view(forward))
            .collect();
        Ok(outcome(ForwardsOutcome::Listed { forwards }))
    }

    /// `forwards.save`: a new forward of `connection`, or the forward `id` changed - stopped
    /// first when it ran, since its port or target changes under it.
    pub(crate) async fn save_forward(
        &self,
        connection: ConnectionId,
        id: Option<i64>,
        draft: &ForwardDraft,
    ) -> Result<Outcome, EngineError> {
        let db = self.db()?;
        let saved = match id {
            Some(id) => {
                let stored = db.forward(id).await?;
                if stored.connection_id != connection {
                    return Err(rc_db::connections::ConnectionError::ForwardNotFound { id }.into());
                }
                self.stop_forward_quietly(id);
                db.update_forward(id, draft).await?
            }
            None => db.create_forward(connection, draft).await?,
        };
        self.announce_forward(&saved, ForwardChange::Changed);
        let forward = self.inner.sessions.forwards.view(saved);
        Ok(outcome(ForwardsOutcome::Forward { forward }))
    }

    /// `forwards.delete`: stopped first when it runs.
    pub(crate) async fn delete_forward(&self, id: i64) -> Result<Outcome, EngineError> {
        let was_running = self.stop_forward_quietly(id).is_some();
        let deleted = self.db()?.delete_forward(id).await?;
        self.announce_forward(&deleted, ForwardChange::Changed);
        Ok(outcome(ForwardsOutcome::Deleted { id, was_running }))
    }

    /// `forwards.start`: binds the forward's listener now; its session is connected by the
    /// first connection that arrives. One that runs already stays as it is.
    ///
    /// # Errors
    ///
    /// [`ForwardError::BindRefused`] for an address other than loopback while
    /// `ssh.forwards_lan` is off (no socket is opened), [`ForwardError::PortInUse`],
    /// [`ForwardError::BindFailed`], the forward missing.
    pub(crate) async fn start_forward(&self, id: i64) -> Result<Outcome, EngineError> {
        let forward = self.db()?.forward(id).await?;
        let forward = self.start(forward).await?;
        Ok(outcome(ForwardsOutcome::Forward { forward }))
    }

    /// `forwards.stop`.
    pub(crate) fn stop_forward(&self, id: i64) -> Outcome {
        let stopped = self.stop_forward_quietly(id).map(|forward| forward.id);
        outcome(ForwardsOutcome::Stopped {
            stopped: stopped.into_iter().collect(),
        })
    }

    /// `forwards.stop_all`: every forward of `connection`, or every one.
    pub(crate) fn stop_all_forwards(&self, connection: Option<ConnectionId>) -> Outcome {
        let stopped = self.stop_forwards(connection);
        outcome(ForwardsOutcome::Stopped { stopped })
    }

    /// Every running forward, by connection and place.
    #[must_use]
    pub fn running_forwards(&self) -> Vec<Forward> {
        self.inner.sessions.forwards.running()
    }

    /// The running forwards the status bar shows: none while `ssh.forwards_in_status_bar` is
    /// off. A call per frame is cheap: a lock and the few running ones.
    #[must_use]
    pub fn status_bar_forwards(&self) -> Vec<Forward> {
        self.inner.sessions.forwards.shown()
    }

    /// Stops the forwards of `connection` (every one for `None`), announced; their ids.
    pub(crate) fn stop_forwards(&self, connection: Option<ConnectionId>) -> Vec<i64> {
        let stopped = self.inner.sessions.forwards.stop_all(connection);
        for forward in &stopped {
            self.announce_forward(forward, ForwardChange::Stopped);
        }
        stopped.into_iter().map(|forward| forward.id).collect()
    }

    /// After the session of `connection` was lost: its forwards stop unless they reconnect
    /// (`ssh.forward_reconnect`).
    pub(crate) async fn forwards_after_loss(&self, connection: ConnectionId) {
        if !self.setting_on(FORWARD_RECONNECT_KEY).await {
            self.stop_forwards(Some(connection));
        }
    }

    /// Starts the forwards of `connection` marked `auto_start` that do not run; a failure is
    /// logged and announced, the session stays.
    ///
    /// Boxed: a session's start awaits it, and a forward's start opens sessions, so the future
    /// names no type of its own (the two would be one recursive type otherwise).
    pub(crate) fn auto_start_forwards(
        &self,
        connection: ConnectionId,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            let Ok(db) = self.db() else {
                return;
            };
            let forwards = match db.forwards(connection).await {
                Ok(forwards) => forwards,
                Err(error) => {
                    tracing::warn!(%connection, code = error.code(), "the port forwards were not read");
                    return;
                }
            };
            for forward in forwards.into_iter().filter(|forward| forward.auto_start) {
                if self.inner.sessions.forwards.active(forward.id).is_some() {
                    continue;
                }
                let id = forward.id;
                if let Err(error) = self.start(forward.clone()).await {
                    tracing::warn!(
                        forward = id,
                        code = error.code(),
                        "a port forward did not start"
                    );
                    let code = error.code().to_owned();
                    self.announce_forward(&forward, ForwardChange::Failed { code });
                }
            }
        })
    }

    /// A `direct-tcpip` channel to `forward`'s target on its connection's session, for the
    /// client at `peer`; connects when the session is gone, and once more when it was lost
    /// between the look and the channel.
    pub(crate) async fn forward_channel(
        &self,
        forward: &Forward,
        peer: SocketAddr,
    ) -> Result<SessionChannel, EngineError> {
        let originator = peer.ip().to_string();
        let port = u32::from(peer.port());
        let host = forward.target_host.as_str();
        let mut retried = false;
        loop {
            let live = self.session(forward.connection_id, true).await?;
            let lease = ChannelLease::new(Arc::clone(&live));
            match live
                .session
                .open_direct_tcpip(host, forward.target_port, &originator, port)
                .await
            {
                Ok(channel) => return Ok(SessionChannel { channel, lease }),
                Err(SshError::ConnectionLost) if !retried => retried = true,
                Err(error) => return Err(error.into()),
            }
        }
    }

    /// Binds and runs `forward`; its state.
    async fn start(&self, forward: Forward) -> Result<ForwardView, EngineError> {
        let desk = &self.inner.sessions.forwards;
        if desk.active(forward.id).is_some() {
            return Ok(desk.view(forward));
        }
        if !is_loopback(&forward.bind_address) && !self.setting_on(FORWARDS_LAN_KEY).await {
            return Err(ForwardError::BindRefused {
                address: forward.bind_address.clone(),
            }
            .into());
        }
        let bound = listener::bind(&forward).await?;
        desk.show(self.setting_on(FORWARDS_IN_STATUS_BAR_KEY).await);
        let stop = CancellationToken::new();
        let active = Arc::new(AtomicUsize::new(0));
        let running = Running {
            forward: forward.clone(),
            stop: stop.clone(),
            active: Arc::clone(&active),
        };
        if !desk.insert(running) {
            // Started by another call meanwhile: this listener goes.
            return Ok(desk.view(forward));
        }
        tracing::info!(forward = forward.id, connection = %forward.connection_id, "port forward started");
        tokio::spawn(listener::serve(
            Arc::downgrade(&self.inner),
            forward.clone(),
            bound,
            stop,
            active,
        ));
        self.announce_forward(&forward, ForwardChange::Started);
        Ok(desk.view(forward))
    }

    /// Stops the forward `id` when it runs, announced; the forward stopped.
    fn stop_forward_quietly(&self, id: i64) -> Option<Forward> {
        let stopped = self.inner.sessions.forwards.stop(id)?;
        self.announce_forward(&stopped, ForwardChange::Stopped);
        Some(stopped)
    }

    fn announce_forward(&self, forward: &Forward, change: ForwardChange) {
        self.publish(EngineEvent::Forward {
            id: forward.id,
            connection: forward.connection_id,
            change,
        });
    }
}

fn outcome(forwards: ForwardsOutcome) -> Outcome {
    Outcome::Forwards(Box::new(forwards))
}

#[cfg(test)]
#[path = "forwards_tests.rs"]
mod tests;
