//! Local port forwards of SSH connections (RC-110-03, `ssh -L`): what runs, what a command
//! answers, what goes wrong.
//!
//! A started forward binds its listener on this computer and keeps it while it runs
//! ([`ForwardDesk`], beside the session pool); every connection it accepts takes the session of
//! its connection from the pool - connecting again when there is none - opens a `direct-tcpip`
//! channel to the target with a [`crate::ChannelLease`] and copies both ways
//! (`forwards/listener.rs`). At most [`MAX_FORWARD_CONNECTIONS`] run through one forward at once;
//! the next is refused and logged. Stopping cancels the listener and every connection on it at
//! once. A listener on an address other than loopback needs `ssh.forwards_lan`.
//!
//! Starting and saving a forward is a person's decision (`forwards.start`, `forwards.save` are
//! `GuiOnly`): even on `127.0.0.1` every process and every user of this computer reaches the
//! port, and through it the target. An agent lists, stops and deletes.

pub(crate) mod listener;

use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::recover::LockOrRecover as _;
use crate::{ConnectionId, Forward};

/// The most connections that run through one forward at once: a bound, not a setting.
pub const MAX_FORWARD_CONNECTIONS: usize = 64;

/// Why a forward did not start.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ForwardError {
    /// The address is not loopback and `ssh.forwards_lan` is off: no socket was opened.
    #[error(
        "{address} is reachable from other computers; switch on ssh.forwards_lan in the settings \
         to listen there"
    )]
    BindRefused { address: String },
    /// Another program listens on the port already.
    #[error("{address} port {port} is in use by another program")]
    PortInUse { address: String, port: u16 },
    /// The port could not be bound for another reason: no rights for a port under 1024, an
    /// address this computer does not have.
    #[error("cannot listen on {address} port {port}: {reason}")]
    BindFailed {
        address: String,
        port: u16,
        reason: String,
    },
}

impl ForwardError {
    /// Every code, in declaration order.
    pub const ALL_CODES: &[&str] = &[
        "forward_bind_refused",
        "forward_port_in_use",
        "forward_bind_failed",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::BindRefused { .. } => "forward_bind_refused",
            Self::PortInUse { .. } => "forward_port_in_use",
            Self::BindFailed { .. } => "forward_bind_failed",
        }
    }
}

impl From<ForwardError> for crate::EngineError {
    fn from(error: ForwardError) -> Self {
        Self::Forward(error)
    }
}

/// Whether `address` is loopback: `127.0.0.1`, `::1` and the rest of `127.0.0.0/8`.
///
/// One of three loopback rules, each for its own boundary and not the same set (RC-110-30): this
/// one judges an IP address a forward binds to; `rc-plugin-host/src/http.rs` and
/// `rc-mcp/src/http.rs` judge a host name and take only `localhost`, `127.0.0.1` and `::1`.
#[must_use]
pub fn is_loopback(address: &str) -> bool {
    address
        .parse::<IpAddr>()
        .is_ok_and(|address| address.is_loopback())
}

/// A forward and whether it runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForwardView {
    #[serde(flatten)]
    pub forward: Forward,
    pub running: bool,
    /// The connections running through it now.
    pub connections: usize,
    /// `127.0.0.1:5432 -> db:5432`.
    pub label: String,
}

/// What a `forwards.*` command answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum ForwardsOutcome {
    /// A connection's forwards in their order, or every running one.
    Listed { forwards: Vec<ForwardView> },
    /// A forward after `forwards.save` or `forwards.start`.
    Forward { forward: ForwardView },
    /// The forwards a stop ended, by id; none when nothing ran.
    Stopped { stopped: Vec<i64> },
    /// A forward deleted, stopped first when it ran.
    Deleted { id: i64, was_running: bool },
}

/// What happened to a forward, for the status bar and the connection manager.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum ForwardChange {
    Started,
    Stopped,
    /// Its fields changed or it was deleted; the list is read again.
    Changed,
    /// An automatic start failed, with the code why.
    Failed {
        code: String,
    },
}

/// The running forwards, by id: kept beside the session pool.
pub(crate) struct ForwardDesk {
    running: Mutex<HashMap<i64, Running>>,
    /// `ssh.forwards_in_status_bar`, read at each start and kept when it changes.
    shown: AtomicBool,
}

impl Default for ForwardDesk {
    fn default() -> Self {
        Self {
            running: Mutex::default(),
            shown: AtomicBool::new(true),
        }
    }
}

/// One running forward.
pub(crate) struct Running {
    pub forward: Forward,
    /// Ends the listener and every connection on it.
    pub stop: CancellationToken,
    /// The connections running through it now.
    pub active: Arc<AtomicUsize>,
}

impl ForwardDesk {
    /// Keeps `running`; `false` when the forward runs already (the new one is not kept).
    pub(crate) fn insert(&self, running: Running) -> bool {
        let mut map = self.running.lock_or_recover("forwards.running");
        if map.contains_key(&running.forward.id) {
            return false;
        }
        map.insert(running.forward.id, running);
        true
    }

    /// Stops the forward `id`; whether it ran.
    pub(crate) fn stop(&self, id: i64) -> Option<Forward> {
        let running = self
            .running
            .lock_or_recover("forwards.running")
            .remove(&id)?;
        running.stop.cancel();
        Some(running.forward)
    }

    /// Stops every forward of `connection`, or every one for `None`; the ones stopped.
    pub(crate) fn stop_all(&self, connection: Option<ConnectionId>) -> Vec<Forward> {
        let mut map = self.running.lock_or_recover("forwards.running");
        let ids: Vec<i64> = map
            .values()
            .filter(|running| connection.is_none_or(|id| running.forward.connection_id == id))
            .map(|running| running.forward.id)
            .collect();
        let mut stopped: Vec<Forward> = ids
            .iter()
            .filter_map(|id| map.remove(id))
            .map(|running| {
                running.stop.cancel();
                running.forward
            })
            .collect();
        stopped.sort_by_key(|forward| forward.id);
        stopped
    }

    /// The connections running through the forward `id`; `None` when it does not run.
    pub(crate) fn active(&self, id: i64) -> Option<usize> {
        self.running
            .lock_or_recover("forwards.running")
            .get(&id)
            .map(|running| running.active.load(Ordering::SeqCst))
    }

    /// Every running forward, by connection and place.
    pub(crate) fn running(&self) -> Vec<Forward> {
        let mut forwards: Vec<Forward> = self
            .running
            .lock_or_recover("forwards.running")
            .values()
            .map(|running| running.forward.clone())
            .collect();
        forwards.sort_by_key(|forward| (forward.connection_id, forward.sort_order, forward.id));
        forwards
    }

    /// Whether the status bar counts the running forwards (`ssh.forwards_in_status_bar`).
    pub(crate) fn show(&self, shown: bool) {
        self.shown.store(shown, Ordering::SeqCst);
    }

    /// The running forwards the status bar counts: none while it does not.
    pub(crate) fn shown(&self) -> Vec<Forward> {
        if self.shown.load(Ordering::SeqCst) {
            self.running()
        } else {
            Vec::new()
        }
    }

    /// `forward` with its state.
    pub(crate) fn view(&self, forward: Forward) -> ForwardView {
        let active = self.active(forward.id);
        ForwardView {
            label: forward.label(),
            running: active.is_some(),
            connections: active.unwrap_or(0),
            forward,
        }
    }
}

#[cfg(test)]
#[path = "forwards_tests.rs"]
mod tests;
