//! The self-update in the engine (RC-100-05 ... RC-100-07): the checks in the background, the
//! verified download, the hand-over to the updater and when an update installs by itself. The
//! formats, the verification and the switch are `rc-update`'s; this keeps what an installation
//! knows (the setting `update.state`) and runs it off the UI thread.
//!
//! * The window starts it once its database is open and its window is up
//!   ([`Engine::start_updates`]): it takes its share of the instance lock, proves an update that
//!   waited for this start, and starts the checks - the first two minutes after the start plus up
//!   to three of jitter, then every `update.check_interval_hours`. The start never waits for a
//!   check, and a check that fails is quiet: its code and time are in `update.status`.
//! * `update.check` checks now, `update.status` answers what is known (both `Read`);
//!   `update.download` and `update.install` are `GuiOnly` - replacing the program is a person's
//!   decision after the release notes, never an agent's.
//! * The floors are stored before anything acts on a check; the offer only ever names a higher
//!   version than the running one, never a pre-release on stable.

mod auto;
mod check;
mod config;
mod handover;
mod status;
mod task;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, atomic::AtomicI64},
};

use rc_update::{
    Fetcher, Installation, Progress, UpdateState,
    auto_install::{AutoMode, IdleClock, Wait},
    install::InstanceLock,
    sign::TrustStore,
};

pub use status::{DownloadState, LastUpdate, REFUSAL_CODES, UpdateRefusal, UpdateStatus};

use crate::{Engine, EngineEvent};

/// The running version, the one every offer is held against.
pub(crate) const RUNNING: &str = crate::about::VERSION;

/// What the engine keeps about the self-update; empty until the window starts it.
#[derive(Default)]
pub(crate) struct UpdateDesk {
    state: Mutex<DeskState>,
    started: OnceLock<Started>,
    /// The tests' fetcher and trust store; the release host and the compiled-in root otherwise.
    fetcher: OnceLock<Arc<dyn Fetcher>>,
    trust: OnceLock<TrustStore>,
    /// When the last key or click arrived, Unix seconds (`when_idle`).
    last_input: AtomicI64,
    /// One check at a time.
    checking: tokio::sync::Mutex<()>,
}

/// What the desk knows now; never held across an `await`.
#[derive(Default)]
struct DeskState {
    stored: UpdateState,
    loaded: bool,
    download: DownloadState,
    progress: Option<Progress>,
    /// The offer's version the notice was put away for ("Later"), until the next check.
    dismissed: Option<String>,
    /// The update was handed to the updater; the window ends now.
    handed: bool,
    idle: IdleClock,
    /// `update.auto_install` as last read, and what `when_idle` waits for.
    auto: AutoMode,
    waiting: Option<Wait>,
    /// "Updated to X" for this start, and the notes of X.
    updated_to: Option<(String, String)>,
    /// The last update failed or was rolled back: its version and reason, for this start.
    failed: Option<(String, String)>,
}

/// Fixed once the window started the self-update.
struct Started {
    data_dir: PathBuf,
    installation: Installation,
    /// This window's share of the instance lock: the updater waits until every window let go.
    _instance: Option<InstanceLock>,
}

/// What the status bar shows about updates, at most one thing at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateNotice {
    /// A newer version is verified: "rCommander X is available"; `on_quit` when it installs at
    /// quitting.
    Available { version: String, on_quit: bool },
    /// This start runs the version an update installed: "Updated to X", with its notes.
    UpdatedTo { version: String, notes: String },
    /// The last update did not go ahead: its version and the reason's code.
    Failed { version: String, code: String },
}

/// The notice for what the desk knows: the update's own result first, then an offer that was
/// not put away. "Updated to X" only while X is the running version (RD-1120-16).
#[must_use]
pub fn notice_of(
    running: &str,
    offer: Option<&rc_update::Offer>,
    dismissed: Option<&str>,
    updated_to: Option<(&str, &str)>,
    failed: Option<(&str, &str)>,
    on_quit: bool,
) -> Option<UpdateNotice> {
    if let Some((version, notes)) = updated_to
        && version == running
    {
        return Some(UpdateNotice::UpdatedTo {
            version: version.to_owned(),
            notes: notes.to_owned(),
        });
    }
    if let Some((version, code)) = failed {
        return Some(UpdateNotice::Failed {
            version: version.to_owned(),
            code: code.to_owned(),
        });
    }
    let offer = offer.filter(|offer| rc_update::is_newer(&offer.version, running))?;
    (dismissed != Some(offer.version.as_str())).then(|| UpdateNotice::Available {
        version: offer.version.clone(),
        on_quit,
    })
}

impl Engine {
    fn desk(&self) -> &UpdateDesk {
        &self.inner.updates
    }

    fn desk_state(&self) -> MutexGuard<'_, DeskState> {
        self.desk()
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// The folder and installation once the window started the self-update.
    fn started(&self) -> Result<&Started, UpdateRefusal> {
        self.desk().started.get().ok_or_else(|| {
            UpdateRefusal::new(
                "update.not_started",
                "the self-update runs in the window only",
            )
        })
    }

    /// What the status bar shows now; read on every redraw, so it touches no disk.
    #[must_use]
    pub fn update_notice(&self) -> Option<UpdateNotice> {
        let on_quit =
            self.desk().started.get().is_some_and(|started| {
                started.installation.kind == rc_update::InstallKind::Portable
            });
        let state = self.desk_state();
        notice_of(
            RUNNING,
            state.stored.offer.as_ref(),
            state.dismissed.as_deref(),
            state
                .updated_to
                .as_ref()
                .map(|(version, notes)| (version.as_str(), notes.as_str())),
            state
                .failed
                .as_ref()
                .map(|(version, code)| (version.as_str(), code.as_str())),
            on_quit && state.auto != AutoMode::Off,
        )
    }

    /// "Later" and the close of "Updated to X": the offer's notice stays away until the next
    /// check; the result of an update was shown once (`check.rs`, `note_update_result`).
    pub fn dismiss_update_notice(&self) {
        {
            let mut state = self.desk_state();
            state.dismissed = state
                .stored
                .offer
                .as_ref()
                .map(|offer| offer.version.clone());
            state.updated_to = None;
            state.failed = None;
        }
        self.publish(EngineEvent::Update);
    }

    /// Changes the stored state and writes it.
    async fn store_update_state(&self, change: impl FnOnce(&mut UpdateState)) {
        let stored = {
            let mut state = self.desk_state();
            change(&mut state.stored);
            state.stored.clone()
        };
        let Ok(db) = self.db() else {
            return;
        };
        if let Err(error) = db.set(rc_update::STATE_KEY, &stored).await {
            tracing::warn!(%error, "the update state could not be stored");
        }
    }

    /// Reads `update.state` once.
    async fn load_update_state(&self) {
        if self.desk_state().loaded {
            return;
        }
        let stored = match self.db() {
            Ok(db) => db
                .get::<serde_json::Value>(rc_update::STATE_KEY)
                .await
                .ok()
                .flatten(),
            Err(_) => None,
        };
        let mut state = self.desk_state();
        if !state.loaded {
            state.stored = UpdateState::from_value(stored);
            state.loaded = true;
        }
    }

    /// A test's fetcher and trust store in place of the release host and the root.
    #[cfg(test)]
    pub(crate) fn use_update_sources(&self, fetcher: Arc<dyn Fetcher>, trust: TrustStore) {
        let _ = self.desk().fetcher.set(fetcher);
        let _ = self.desk().trust.set(trust);
    }
}

#[cfg(test)]
#[path = "updates_tests.rs"]
mod tests;
