//! `app.quit`: F10 without a dialog asks the window to end the app (RC-080-17). The window is
//! the window's, so the engine only announces it, with the number of jobs that run or wait when
//! the person wants to be asked about them (`app.confirm_quit_with_transfers`). Unfinished jobs
//! are kept for the next start either way.

use crate::{Engine, EngineEvent, Outcome, settings::CONFIRM_QUIT_KEY};

impl Engine {
    /// `app.quit`: announces the wish to quit; answers with the queue as it is.
    pub(crate) async fn quit(&self) -> Outcome {
        let jobs = self.inner.ops.jobs();
        let running = jobs.iter().filter(|job| !job.state.is_finished()).count();
        let unfinished = if running > 0 && self.setting_on(CONFIRM_QUIT_KEY).await {
            running
        } else {
            0
        };
        self.publish(EngineEvent::Quit { unfinished });
        self.queue_list()
    }
}

#[cfg(test)]
#[path = "quit_tests.rs"]
mod tests;
