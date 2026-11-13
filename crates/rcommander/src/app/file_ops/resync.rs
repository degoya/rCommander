//! The jobs after missed events (RC-110-27). The engine's event stream is bounded; a window that
//! fell behind - a modal loop of the system held its thread while a copy reported progress - is
//! told so once (`Message::EngineLagged`) and never hears what was dropped. So everything the
//! window keeps about jobs is read again from the queue itself:
//!
//! - the background bar and the open queue window, with the saved lists;
//! - every job the window follows: one the queue has ended (or forgotten) ends here as its
//!   `Finished` would have ended it - for the progress window, the panels, the multi-rename
//!   dialog, a drag's fetch, the checksum list and the archive test's line -, the others take
//!   the queue's state, progress and open question, so a conflict or a failing file asked
//!   unheard is asked now;
//! - the multi-rename dialog's run, should the window follow it no longer;
//! - an archive's password asked unheard.
//!
//! The stream goes on behind the gap, so the `Finished` of a job ended here may still arrive: it
//! is dropped once ([`FileOps::ended_unheard`]), so nothing is reported twice.

use rc_engine::{JobChange, JobId, JobState};

use crate::app::{App, Effect};

impl App {
    /// A change of a job the queue announced, for everything in the window that follows jobs.
    pub(in crate::app) fn job_changed(&mut self, job: JobId, change: JobChange) -> Vec<Effect> {
        if matches!(change, JobChange::Finished { .. }) {
            let unheard = &mut self.file_ops.ended_unheard;
            if let Some(at) = unheard.iter().position(|ended| *ended == job) {
                // Ended already after missed events; the late event only shows the queue anew.
                unheard.swap_remove(at);
                self.refresh_queue_window();
                return Vec::new();
            }
        }
        self.archive_test_result(job, &change);
        self.checksum_result(job, &change);
        let renamed = self.multi_rename_job(job, &change);
        let mut effects = self.drag_job(job, &change);
        effects.extend(self.on_job(job, change));
        effects.extend(renamed);
        effects
    }

    /// After missed events: the queue, the jobs the window follows and their questions, read
    /// again from the engine.
    pub(in crate::app) fn resync_jobs(&mut self) -> Vec<Effect> {
        self.refresh_queue_window();
        let mut effects = self.read_saved_lists();
        // A job the queue has forgotten sends no `Finished` any more.
        let ops = self.engine.ops();
        self.file_ops
            .ended_unheard
            .retain(|job| ops.job(*job).is_ok());
        let followed: Vec<JobId> = self
            .file_ops
            .jobs
            .snapshots()
            .map(|snapshot| snapshot.id)
            .collect();
        let mut ended = Vec::new();
        let mut running = Vec::new();
        for job in followed {
            match self.engine.ops().job(job) {
                Ok(snapshot) if snapshot.state.is_finished() => ended.push((job, snapshot.state)),
                Ok(snapshot) => {
                    let _ = self.file_ops.jobs.refresh(snapshot);
                    running.push(job);
                }
                // The queue knows it no longer: over, how is not known - nothing is claimed done.
                Err(_) => ended.push((job, JobState::Cancelled)),
            }
        }
        for (job, state) in ended {
            let known = self.engine.ops().job(job).is_ok();
            effects.extend(self.job_changed(job, JobChange::Finished { state }));
            if known {
                self.file_ops.ended_unheard.push(job);
            }
        }
        // A failing file asked unheard of a job told "skip all" is skipped as any other.
        for job in running {
            effects.extend(self.skip_if_asked(job));
        }
        self.show_device_hints();
        effects.extend(self.multi_rename_resync());
        // An archive's password asked unheard, unless an archive dialog is open: then it waits
        // for that to close, as always.
        self.ask_waiting_password();
        effects
    }
}

#[cfg(test)]
#[path = "resync_tests.rs"]
mod tests;
