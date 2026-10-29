//! Locking the vault when the person leaves the machine: the system's session lock (Win+L, a lock
//! screen) and suspend (sleep, hibernation) - RC-080-11, V2.
//!
//! The system's notices arrive as [`SessionEvent`]s on one channel for the whole process
//! ([`subscribe`]). On Windows a hidden window on a thread of its own receives them
//! (`session_lock/windows.rs`); elsewhere none arrives yet. Every engine with a vault listens
//! ([`Engine::attach_vault`]) and locks it while the setting [`LOCK_ON_SESSION_LOCK_KEY`] is on -
//! the default, protected like every `vault.*` key, so no agent switches it off.

use std::sync::{OnceLock, Weak};

use rc_vault::Vault;
use tokio::sync::broadcast::{self, error::RecvError};

use crate::{Engine, EngineEvent, Inner};

#[cfg(windows)]
mod windows;

/// The setting: lock the vault when the session locks or the machine suspends.
pub const LOCK_ON_SESSION_LOCK_KEY: &str = "vault.lock_on_session_lock";

/// The person left: what the system tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEvent {
    /// The session locked.
    Locked,
    /// The machine is about to sleep or hibernate.
    Suspending,
}

/// Win32's numbers for the two notices (`WM_WTSSESSION_CHANGE` with `WTS_SESSION_LOCK`,
/// `WM_POWERBROADCAST` with `PBT_APMSUSPEND`), here so [`classify`] is tested on every system;
/// `session_lock/windows.rs` holds them against `windows-sys`'s.
const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
const WTS_SESSION_LOCK: usize = 0x7;
const WM_POWERBROADCAST: u32 = 0x0218;
const PBT_APMSUSPEND: usize = 0x4;

/// The notices waiting for listeners; how many a slow one may miss before it locks anyway.
const SESSION_BUFFER: usize = 8;

static EVENTS: OnceLock<broadcast::Sender<SessionEvent>> = OnceLock::new();

/// What a window message says about the session, when it says anything.
#[must_use]
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "the Windows watcher calls it; tested everywhere")
)]
pub(crate) fn classify(message: u32, wparam: usize) -> Option<SessionEvent> {
    match (message, wparam) {
        (WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK) => Some(SessionEvent::Locked),
        (WM_POWERBROADCAST, PBT_APMSUSPEND) => Some(SessionEvent::Suspending),
        _ => None,
    }
}

/// A receiver of the system's notices from now on; the first call starts watching for them.
pub(crate) fn subscribe() -> broadcast::Receiver<SessionEvent> {
    EVENTS
        .get_or_init(|| {
            let (sender, _) = broadcast::channel(SESSION_BUFFER);
            #[cfg(windows)]
            windows::watch(sender.clone());
            sender
        })
        .subscribe()
}

impl Engine {
    /// The person left: locks the vault while [`LOCK_ON_SESSION_LOCK_KEY`] is on and announces
    /// it; says whether it locked.
    pub async fn on_session_event(&self, event: SessionEvent) -> bool {
        if !self.setting_on(LOCK_ON_SESSION_LOCK_KEY).await {
            return false;
        }
        let locked = self.inner.vault.get().is_some_and(Vault::lock);
        if locked {
            tracing::debug!(?event, "the vault locked with the session");
            self.publish(EngineEvent::VaultLocked);
        }
        locked
    }
}

/// Hands every notice of `events` to the engine, for as long as it lives. Holds the engine only
/// while handling one, so dropping the last [`Engine`] ends it at the next notice.
pub(crate) async fn listen(inner: Weak<Inner>, mut events: broadcast::Receiver<SessionEvent>) {
    loop {
        let event = match events.recv().await {
            Ok(event) => event,
            // Missed notices still mean the person left.
            Err(RecvError::Lagged(_)) => SessionEvent::Locked,
            Err(RecvError::Closed) => return,
        };
        let Some(inner) = inner.upgrade() else {
            return;
        };
        Engine { inner }.on_session_event(event).await;
    }
}

#[cfg(test)]
#[path = "session_lock_tests.rs"]
mod tests;
