//! The SSH sessions of the connections: one per connection, shared by every channel on it.
//!
//! A session is opened on first use (`connections.connect`, or [`crate::Engine::ssh_channel`]
//! for SFTP, a terminal or a command) and kept in the pool. Each channel holds a
//! [`ChannelLease`]; a session without one for the connection's idle time is closed. A session
//! the keepalive gives up on, or the server closes, is announced as lost
//! ([`SessionChange::Lost`]) and dropped from the pool, and the next use connects again: the
//! secrets come from the vault again, without a question while it is unlocked.
//!
//! The settings of a connection ([`SessionPrefs`]: auth order, agent key, keepalive, idle close)
//! live in `rcommander.db` (migration 0010); a connection without them takes the defaults.

mod sources;

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use rc_db::StoredSessionPrefs;
use rc_remote::{
    OfferedKey, SshSession,
    ssh::{AuthKind, AuthUsed, SshChannel, format_order, parse_order},
};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::{ConnectionId, EngineError};

pub(crate) use sources::{EngineCredentials, EngineGate};

/// The keepalive interval of a connection without its own, in seconds.
pub const DEFAULT_KEEPALIVE_SECS: u32 = 30;
/// The idle time after which a session without channels closes, in seconds.
pub const DEFAULT_IDLE_CLOSE_SECS: u32 = 600;
/// The longest keepalive interval a connection may set, in seconds.
pub const MAX_KEEPALIVE_SECS: u32 = 3600;
/// The longest idle time a connection may set, in seconds.
pub const MAX_IDLE_CLOSE_SECS: u32 = 86_400;

/// A connection's SSH session settings; a field left out of a command takes its default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPrefs {
    /// The methods to try, in order; one without what it needs is skipped.
    #[serde(default = "default_order")]
    pub auth_order: Vec<AuthKind>,
    /// Offer only the agent key with this `SHA256:` fingerprint; every key when `None`.
    #[serde(default)]
    pub agent_fingerprint: Option<String>,
    /// 1 to [`MAX_KEEPALIVE_SECS`].
    #[serde(default = "default_keepalive")]
    pub keepalive_secs: u32,
    /// 0 keeps an unused session open; at most [`MAX_IDLE_CLOSE_SECS`].
    #[serde(default = "default_idle_close")]
    pub idle_close_secs: u32,
}

fn default_order() -> Vec<AuthKind> {
    AuthKind::DEFAULT_ORDER.to_vec()
}

fn default_keepalive() -> u32 {
    DEFAULT_KEEPALIVE_SECS
}

fn default_idle_close() -> u32 {
    DEFAULT_IDLE_CLOSE_SECS
}

impl Default for SessionPrefs {
    fn default() -> Self {
        Self {
            auth_order: default_order(),
            agent_fingerprint: None,
            keepalive_secs: DEFAULT_KEEPALIVE_SECS,
            idle_close_secs: DEFAULT_IDLE_CLOSE_SECS,
        }
    }
}

impl SessionPrefs {
    /// The settings checked and in their stored form.
    pub(crate) fn checked(&self) -> Result<StoredSessionPrefs, EngineError> {
        let invalid = |reason: &str| EngineError::SessionPrefsInvalid {
            reason: reason.to_owned(),
        };
        if self.auth_order.is_empty() {
            return Err(invalid("auth_order names no method"));
        }
        let stored_order = format_order(&self.auth_order);
        if parse_order(&stored_order).as_ref() != Some(&self.auth_order) {
            return Err(invalid("auth_order names a method twice"));
        }
        if !(1..=MAX_KEEPALIVE_SECS).contains(&self.keepalive_secs) {
            return Err(invalid("keepalive_secs is 1 to 3600"));
        }
        if self.idle_close_secs > MAX_IDLE_CLOSE_SECS {
            return Err(invalid("idle_close_secs is 0 (never) to 86400"));
        }
        let fingerprint = self
            .agent_fingerprint
            .as_deref()
            .map(str::trim)
            .filter(|fingerprint| !fingerprint.is_empty());
        if fingerprint.is_some_and(|fingerprint| !is_fingerprint(fingerprint)) {
            return Err(invalid("agent_fingerprint is a SHA256: fingerprint"));
        }
        Ok(StoredSessionPrefs {
            auth_order: stored_order,
            agent_fingerprint: fingerprint.map(str::to_owned),
            keepalive_secs: self.keepalive_secs,
            idle_close_secs: self.idle_close_secs,
        })
    }

    /// The stored settings; an order that no longer parses falls back to the default.
    pub(crate) fn from_stored(stored: StoredSessionPrefs) -> Self {
        let auth_order = parse_order(&stored.auth_order).unwrap_or_else(|| {
            tracing::warn!("a connection's stored auth order is not valid; the default is used");
            default_order()
        });
        Self {
            auth_order,
            agent_fingerprint: stored.agent_fingerprint,
            keepalive_secs: stored.keepalive_secs.clamp(1, MAX_KEEPALIVE_SECS),
            idle_close_secs: stored.idle_close_secs.min(MAX_IDLE_CLOSE_SECS),
        }
    }
}

/// `SHA256:` and unpadded base64, as `ssh-keygen -l` prints a fingerprint.
fn is_fingerprint(text: &str) -> bool {
    text.strip_prefix("SHA256:").is_some_and(|digest| {
        (1..=64).contains(&digest.len())
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/')
    })
}

/// Where a connection's session stands; a connection without one is not listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SshState {
    Connecting,
    Connected,
}

/// A connection's session as commands and the connection manager show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStatus {
    pub connection_id: ConnectionId,
    pub state: SshState,
    /// How the login got in; `None` while connecting.
    pub login: Option<AuthUsed>,
    /// The server's host-key fingerprint; `None` while connecting.
    pub host_key_fingerprint: Option<String>,
    /// The channels open on it.
    pub channels: usize,
}

/// What `connections.test` found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTestReport {
    pub connection_id: ConnectionId,
    pub login: AuthUsed,
    pub host_key: OfferedKey,
    /// Connect, handshake and login, in milliseconds.
    pub duration_ms: u64,
}

/// What happened to a connection's session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionChange {
    Connected,
    /// Closed on purpose: `connections.disconnect`, or idle without channels.
    Closed,
    /// Gone without being asked: the server closed it or stopped answering.
    Lost,
}

/// A channel on a connection's session, with the lease that keeps the session counted as in
/// use. Drop the lease when the channel is done.
pub struct SessionChannel {
    pub channel: SshChannel,
    pub lease: ChannelLease,
}

/// One channel's share of a session.
pub struct ChannelLease {
    live: Arc<Live>,
    /// Set by [`ChannelLease::end_unused`]: when the channel was last used.
    unused_since: Option<Instant>,
}

impl ChannelLease {
    pub(crate) fn new(live: Arc<Live>) -> Self {
        live.channels.fetch_add(1, Ordering::SeqCst);
        Self {
            live,
            unused_since: None,
        }
    }

    /// The session the channel runs on.
    pub(crate) fn session(&self) -> &SshSession {
        &self.live.session
    }

    /// How long the session may go without a channel before it closes; `None`: never.
    pub(crate) fn idle_close(&self) -> Option<Duration> {
        self.live.idle_close
    }

    /// Ends the lease of a channel nobody used since `since` (an idle SFTP backend, RC-080-51):
    /// the session counts as idle from then, or from the end of its last other channel when that
    /// came later, so it closes after its idle time and not that long once more.
    pub(crate) fn end_unused(mut self, since: Instant) {
        self.unused_since = Some(since);
        drop(self);
    }
}

impl Drop for ChannelLease {
    fn drop(&mut self) {
        let ended = match self.unused_since {
            Some(since) => since.max(*lock(&self.live.last_end)),
            None => {
                let now = Instant::now();
                *lock(&self.live.last_end) = now;
                now
            }
        };
        if self.live.channels.fetch_sub(1, Ordering::SeqCst) == 1 {
            *lock(&self.live.idle_since) = ended;
        }
    }
}

/// An open session in the pool.
pub(crate) struct Live {
    pub session: SshSession,
    channels: AtomicUsize,
    idle_since: Mutex<Instant>,
    /// When the last lease that was in use ended, or the session came up.
    last_end: Mutex<Instant>,
    /// `None`: never closed for being idle.
    pub idle_close: Option<Duration>,
    /// Set when the session is closed on purpose, so its end is not reported as lost.
    pub closing: AtomicBool,
}

impl Live {
    pub(crate) fn new(session: SshSession, idle_close_secs: u32) -> Self {
        Self {
            session,
            channels: AtomicUsize::new(0),
            idle_since: Mutex::new(Instant::now()),
            last_end: Mutex::new(Instant::now()),
            idle_close: (idle_close_secs > 0)
                .then(|| Duration::from_secs(u64::from(idle_close_secs))),
            closing: AtomicBool::new(false),
        }
    }

    /// How long no channel has been open; `None` while one is.
    pub(crate) fn idle_for(&self) -> Option<Duration> {
        (self.channels.load(Ordering::SeqCst) == 0).then(|| lock(&self.idle_since).elapsed())
    }

    pub(crate) fn status(&self, id: ConnectionId) -> SessionStatus {
        SessionStatus {
            connection_id: id,
            state: SshState::Connected,
            login: Some(self.session.auth().clone()),
            host_key_fingerprint: Some(self.session.host_key().fingerprint.clone()),
            channels: self.channels.load(Ordering::SeqCst),
        }
    }
}

/// The pool: one slot per connection with a session, a setup under way, or a caller holding it.
/// A slot with none of these is dropped when another is taken (RC-080-47), so the pool does not
/// grow with every connection ever used.
#[derive(Default)]
pub(crate) struct SessionPool {
    slots: Mutex<HashMap<ConnectionId, Arc<Slot>>>,
}

/// One connection's place in the pool.
#[derive(Default)]
pub(crate) struct Slot {
    /// Serializes connecting: a second caller waits and takes the first one's session.
    pub turn: tokio::sync::Mutex<()>,
    state: Mutex<SlotState>,
}

#[derive(Default)]
struct SlotState {
    live: Option<Arc<Live>>,
    /// Cancelled by `connections.disconnect`; every setup runs on a child of it.
    cancel: CancellationToken,
    /// Setups under way.
    connecting: usize,
}

impl SessionPool {
    pub(crate) fn slot(&self, id: ConnectionId) -> Arc<Slot> {
        let mut slots = lock(&self.slots);
        // Only the pool holds a slot nobody uses: no caller can take it meanwhile, the map being
        // locked.
        slots.retain(|other, slot| *other == id || Arc::strong_count(slot) > 1 || !slot.idle());
        Arc::clone(slots.entry(id).or_default())
    }

    /// Every connection with a session or a setup under way, by id.
    pub(crate) fn statuses(&self) -> Vec<SessionStatus> {
        let slots: Vec<(ConnectionId, Arc<Slot>)> = lock(&self.slots)
            .iter()
            .map(|(id, slot)| (*id, Arc::clone(slot)))
            .collect();
        let mut statuses: Vec<SessionStatus> = slots
            .iter()
            .filter_map(|(id, slot)| slot.status(*id))
            .collect();
        statuses.sort_by_key(|status| status.connection_id);
        statuses
    }

    pub(crate) fn status(&self, id: ConnectionId) -> Option<SessionStatus> {
        let slot = lock(&self.slots).get(&id).cloned()?;
        slot.status(id)
    }
}

impl Slot {
    fn state(&self) -> MutexGuard<'_, SlotState> {
        lock(&self.state)
    }

    /// No session up and no setup under way.
    fn idle(&self) -> bool {
        let state = self.state();
        state.connecting == 0
            && state
                .live
                .as_ref()
                .is_none_or(|live| live.session.is_closed())
    }

    fn status(&self, id: ConnectionId) -> Option<SessionStatus> {
        let state = self.state();
        match &state.live {
            Some(live) if !live.session.is_closed() => Some(live.status(id)),
            _ if state.connecting > 0 => Some(SessionStatus {
                connection_id: id,
                state: SshState::Connecting,
                login: None,
                host_key_fingerprint: None,
                channels: 0,
            }),
            _ => None,
        }
    }

    /// The session, while it is up.
    pub(crate) fn live(&self) -> Option<Arc<Live>> {
        self.state()
            .live
            .as_ref()
            .filter(|live| !live.session.is_closed())
            .cloned()
    }

    pub(crate) fn set_live(&self, live: Arc<Live>) {
        self.state().live = Some(live);
    }

    /// Drops `live` from the slot, unless another session took its place meanwhile.
    pub(crate) fn forget(&self, live: &Arc<Live>) {
        let mut state = self.state();
        if state
            .live
            .as_ref()
            .is_some_and(|kept| Arc::ptr_eq(kept, live))
        {
            state.live = None;
        }
    }

    /// A token for one setup, cancelled by [`Slot::close`]; counted as connecting until the
    /// guard drops.
    pub(crate) fn begin_setup(self: &Arc<Self>) -> (CancellationToken, SetupGuard) {
        let mut state = self.state();
        state.connecting += 1;
        (
            state.cancel.child_token(),
            SetupGuard {
                slot: Arc::clone(self),
            },
        )
    }

    /// Cancels every setup under way and takes the session out; says whether there was
    /// either.
    pub(crate) fn close(&self) -> (bool, Option<Arc<Live>>) {
        let mut state = self.state();
        let connecting = state.connecting > 0;
        state.cancel.cancel();
        state.cancel = CancellationToken::new();
        let live = state.live.take();
        if let Some(live) = &live {
            live.closing.store(true, Ordering::SeqCst);
        }
        (connecting, live)
    }
}

/// Counts a setup as under way while it lives.
pub(crate) struct SetupGuard {
    slot: Arc<Slot>,
}

impl Drop for SetupGuard {
    fn drop(&mut self) {
        let mut state = self.slot.state();
        state.connecting = state.connecting.saturating_sub(1);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
