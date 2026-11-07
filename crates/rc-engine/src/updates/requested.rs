//! "Install and restart" before the download is there (RC-100-84): one button for one intent. The
//! click starts the download, or joins the one running, and the engine remembers it in the desk,
//! so closing the settings changes nothing. Once the file is downloaded and checked, the engine
//! hands it over as `update.install` does and asks the window to end; a running, waiting or
//! paused job holds it back until the queue is empty. A failed download drops the intent - the
//! person asks again. Quitting never waits for it: the window ends without the update.

use std::time::Duration;

use super::{DownloadState, UpdateRefusal};
use crate::{Engine, EngineEvent};

/// How often a requested install looks whether the queue is empty.
const QUEUE_LOOK: Duration = Duration::from_secs(1);

impl Engine {
    /// Remembers that the person asked to install and starts the download; a running download is
    /// joined. Only a portable installation replaces itself.
    pub(super) fn install_after_download(&self) -> Result<(), UpdateRefusal> {
        self.installs_itself()?;
        let running = {
            let mut state = self.desk_state();
            if state.handed {
                return Err(UpdateRefusal::new(
                    "update.handed",
                    "the update is handed to the updater already",
                ));
            }
            state.install_requested = true;
            matches!(state.download, DownloadState::Running { .. })
        };
        if !running {
            let started = self.start_update_download();
            // A download running now - this one, or a check's started meanwhile - installs at
            // its end: the intent was set before it.
            if !self.download_running() {
                if let Err(refusal) = started {
                    self.desk_state().install_requested = false;
                    return Err(refusal);
                }
                // The verified file was there already, or the download ended at once.
                let engine = self.clone();
                tokio::spawn(async move { engine.finish_requested_install().await });
            }
        }
        self.publish(EngineEvent::Update);
        Ok(())
    }

    fn download_running(&self) -> bool {
        matches!(self.desk_state().download, DownloadState::Running { .. })
    }

    /// The download a person asked to install is ready: waits for the empty queue, then hands it
    /// over and asks the window to end. A refused hand-over shows as the failed update's notice.
    pub(super) async fn finish_requested_install(&self) {
        loop {
            {
                let state = self.desk_state();
                let ready = matches!(state.download, DownloadState::Ready { .. });
                if !state.install_requested || state.handed || !ready {
                    return;
                }
            }
            if !self.jobs_unfinished() {
                break;
            }
            tokio::time::sleep(QUEUE_LOOK).await;
        }
        // Taken once: a second look that raced this one finds nothing to do.
        if !std::mem::take(&mut self.desk_state().install_requested) {
            return;
        }
        match self.hand_over_update(true).await {
            // The window ends as `app.quit` ends it; the queue is empty.
            Ok(()) => self.publish(EngineEvent::Quit { unfinished: 0 }),
            Err(refusal) => {
                tracing::warn!(code = refusal.code(), %refusal, "the requested update was not installed");
                {
                    let mut state = self.desk_state();
                    let version = state
                        .stored
                        .offer
                        .as_ref()
                        .map(|offer| offer.version.clone())
                        .unwrap_or_default();
                    state.failed = Some((version, refusal.code().to_owned()));
                }
                self.publish(EngineEvent::Update);
            }
        }
    }
}

#[cfg(test)]
#[path = "requested_tests.rs"]
mod tests;
