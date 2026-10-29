//! When an offer installs by itself (RC-100-07): `on_quit` hands it to the updater as the window
//! ends and leaves the new version to the next start; `when_idle` also restarts into it once
//! nothing has happened for `update.idle_minutes`. The decision is `rc_update::auto_install`'s;
//! this gathers what it decides on: the window's last input and its dialogs, the queue, the
//! terminals and the SSH sessions.

use std::sync::{PoisonError, atomic::Ordering};

use chrono::{DateTime, TimeDelta, Utc};
use rc_update::auto_install::{self, Moment, Wait};

use super::{DownloadState, RUNNING};
use crate::{Engine, EngineEvent};

impl Engine {
    /// A key or a click arrived: the quiet of `when_idle` starts again.
    pub fn note_input(&self) {
        self.desk()
            .last_input
            .store(Utc::now().timestamp(), Ordering::Relaxed);
    }

    /// Whether a terminal or an SSH session is open.
    fn sessions_open(&self) -> bool {
        let terminals = !self
            .inner
            .terminals
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty();
        terminals || !self.inner.sessions.statuses().is_empty()
    }

    /// The moment the decision looks at, without the window's dialogs.
    async fn auto_moment(&self) -> (auto_install::AutoMode, Option<String>, bool, bool) {
        let config = self.update_config().await;
        let installs_itself =
            self.desk().started.get().is_some_and(|started| {
                started.installation.kind == rc_update::InstallKind::Portable
            });
        let state = self.desk_state();
        let ready = matches!(state.download, DownloadState::Ready { .. })
            .then(|| {
                state
                    .stored
                    .offer
                    .as_ref()
                    .map(|offer| offer.version.clone())
            })
            .flatten();
        let failed = ready
            .as_deref()
            .is_some_and(|version| state.stored.failed_before(version));
        (config.auto, ready, installs_itself, failed)
    }

    /// The window ends (`app.quit`, its X, Alt+F4): with `on_quit` or `when_idle` and a
    /// downloaded offer, the updater installs it without starting it. Whether it was handed over.
    pub async fn update_on_quit(&self) -> bool {
        let (mode, ready, installs_itself, failed_before) = self.auto_moment().await;
        let moment = Moment {
            mode,
            installs_itself,
            ready: ready.as_deref(),
            failed_before,
            jobs: self.jobs_unfinished(),
            ..Moment::default()
        };
        if auto_install::on_quit(&moment).is_err() || self.desk_state().handed {
            return false;
        }
        match self.hand_over_update(false).await {
            Ok(()) => true,
            Err(refusal) => {
                tracing::warn!(code = refusal.code(), %refusal, "the update was not installed at quitting");
                false
            }
        }
    }

    /// One look of `when_idle`, every half minute from the window: `dialog_open` is whether a
    /// dialog is open. When the quiet has lasted, the update is handed over, the window is asked
    /// to end, and this answers `true`.
    pub async fn update_idle_tick(&self, dialog_open: bool) -> bool {
        let (mode, ready, installs_itself, failed_before) = self.auto_moment().await;
        let idle = TimeDelta::minutes(i64::from(self.update_config().await.idle_minutes));
        let last_input =
            DateTime::from_timestamp(self.desk().last_input.load(Ordering::Relaxed), 0)
                .filter(|at| at.timestamp() > 0);
        let moment = Moment {
            mode,
            installs_itself,
            ready: ready.as_deref(),
            failed_before,
            last_input,
            jobs: self.jobs_unfinished(),
            sessions: self.sessions_open(),
            dialog: dialog_open,
        };
        let decided = {
            let mut state = self.desk_state();
            let decided = state.idle.observe(&moment, idle, Utc::now());
            state.waiting = decided.err();
            decided
        };
        if decided.is_err() || self.desk_state().handed {
            return false;
        }
        match self.hand_over_update(true).await {
            Ok(()) => {
                tracing::info!(
                    version = RUNNING,
                    "rCommander restarts into the update while idle"
                );
                self.publish(EngineEvent::Quit { unfinished: 0 });
                true
            }
            Err(refusal) => {
                // Never again by itself for this version: a refused hand-over is a failure.
                self.desk_state().waiting = Some(Wait::FailedBefore);
                if let Some(version) = ready {
                    self.store_update_state(|stored| stored.record_failure(&version))
                        .await;
                }
                tracing::warn!(code = refusal.code(), %refusal, "the idle update was not installed");
                false
            }
        }
    }
}
