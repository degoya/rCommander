//! A job's way through the window: adopted when the engine took it, moved on by the queue's
//! events, answered by itself where "skip all" was chosen, and when it is over both panels read
//! again, the copied sources deselected, the cursor on what was made or renamed. A move's
//! sources are deselected when it starts: they leave the panel anyway (owner, 2026-10-19).

use std::collections::HashMap;

use rc_engine::{
    Answer, Command, ErrorChoice, Failure, JobChange, JobId, JobKind, JobState, PanelId, Question,
};

use super::{
    FollowUp, Submit, Submitted,
    dialog::cursor_on,
    jobs::{self, Tracked},
};
use crate::{
    app::{App, Effect},
    i18n::{error_text, t},
};

/// The commands that follow a finished operation of `panel`; `done` when it went through, so
/// the cursor may go to what it made.
pub fn follow_commands(panel: PanelId, follow: &FollowUp, done: bool) -> Vec<Command> {
    let refresh = |panel| Command::Refresh { panel: Some(panel) };
    match follow {
        FollowUp::Transfer { .. } | FollowUp::Refresh => {
            vec![refresh(panel), refresh(panel.other())]
        }
        FollowUp::Cursor(name) => {
            let mut commands = vec![refresh(panel)];
            if let (true, Some(name)) = (done, name) {
                commands.push(Command::SetCursor {
                    panel: Some(panel),
                    target: cursor_on(name),
                });
            }
            commands
        }
    }
}

impl App {
    /// The engine took a job, or the jobs of one operation (or refused it): the window follows
    /// each from the queue's own snapshot, which holds what the job did and asked before this
    /// message came. Of several, the first that has not finished goes into the progress window,
    /// the next ones follow it there (`jobs.rs`). One a locked vault refused goes to the unlock
    /// prompt, which submits it again once the vault is open.
    pub(super) fn submitted(&mut self, submitted: Submitted) -> Vec<Effect> {
        let Submitted {
            panel,
            follow,
            background,
            result,
            refused,
        } = submitted;
        if let Some(command) = refused {
            return self.ops_refused(Submit {
                panel,
                command,
                follow,
                background,
            });
        }
        let snapshots = match result {
            Ok(snapshots) => snapshots,
            Err(code) => {
                self.pane_mut(panel).error = Some(error_text(code));
                return Vec::new();
            }
        };
        let mut effects = Vec::new();
        if follow == (FollowUp::Transfer { deselect: false }) {
            let kinds = snapshots.iter().map(|snapshot| &snapshot.kind);
            let names = self.selected_sources(panel, kinds, &[]);
            if !names.is_empty() {
                let deselect = Command::ToggleSelection {
                    panel: Some(panel),
                    names,
                };
                effects.push(Effect::Dispatch(panel, vec![deselect]));
            }
        }
        let mut foreground = !background;
        let (mut adopted, mut finished) = (Vec::new(), Vec::new());
        // A job just adopted has been told no "skip all" yet: nothing to answer for it here.
        for snapshot in snapshots {
            let id = snapshot.id;
            let snapshot = self.engine.ops().job(id).unwrap_or(snapshot);
            let jobs = &mut self.file_ops.jobs;
            if let Some(done) = jobs.adopt_unmeasured(snapshot, panel, follow.clone(), foreground) {
                finished.push(done);
                continue;
            }
            if foreground {
                jobs.set_foreground(id, true);
                foreground = false;
            }
            adopted.push(id);
        }
        // The pace once per operation, the panels read again once per batch (RC-080-45).
        self.file_ops.jobs.measure_adopted(&adopted);
        let mut last_of: HashMap<u64, usize> = HashMap::new();
        for (at, done) in finished.iter().enumerate() {
            if let Some(batch) = done.snapshot.batch {
                last_of.insert(batch, at);
            }
        }
        for (at, done) in finished.into_iter().enumerate() {
            let later = done
                .snapshot
                .batch
                .is_some_and(|batch| last_of.get(&batch) != Some(&at));
            effects.extend(self.job_finished(done, later));
        }
        effects
    }

    /// Pauses, resumes or cancels job `job` - in the progress window its whole operation, every
    /// job of its batch (RC-010-14); the queue window steers each job on its own.
    pub(super) fn steer_all(&self, job: JobId, command: fn(JobId) -> Command) -> Vec<Effect> {
        let jobs = &self.file_ops.jobs;
        let Some(tracked) = jobs.get(job) else {
            return vec![Effect::Dispatch(self.active, vec![command(job)])];
        };
        let ids = if tracked.foreground {
            jobs.siblings(job)
        } else {
            vec![job]
        };
        vec![Effect::Dispatch(
            tracked.panel,
            ids.into_iter().map(command).collect(),
        )]
    }

    /// A change of a job the queue announced.
    pub(in crate::app) fn on_job(&mut self, job: JobId, change: JobChange) -> Vec<Effect> {
        match change {
            // Progress moves no job in the queue's order: the bar's and the window's row of the
            // job take its progress, the queue is not read again (RC-080-45).
            JobChange::Progress { .. } => self.progress_rows(job),
            _ => self.refresh_queue_window(),
        }
        match self.file_ops.jobs.apply(job, change) {
            Some(mut done) => {
                // The events carry no failures: the queue's own snapshot of the ended job has
                // them, for the status line.
                if let Ok(snapshot) = self.engine.ops().job(job) {
                    done.snapshot = snapshot;
                }
                self.job_finished(done, false)
            }
            None => self.skip_if_asked(job),
        }
    }

    /// A failing file of a job that was told "skip all" is skipped without asking.
    fn skip_if_asked(&self, job: JobId) -> Vec<Effect> {
        let Some(tracked) = self.file_ops.jobs.get(job) else {
            return Vec::new();
        };
        if !(tracked.skip_errors && matches!(tracked.snapshot.question, Some(Question::Error(_)))) {
            return Vec::new();
        }
        let answer = Answer::Error {
            choice: ErrorChoice::Skip,
        };
        vec![Effect::Dispatch(
            tracked.panel,
            vec![Command::OpsAnswer { job, answer }],
        )]
    }

    /// A job is over: what failed goes into the panel's status line, a copy's sources that made
    /// it are deselected, and once the last job of its batch is over - `later` when one more of
    /// it is still to be handled - both panels are read again (RC-080-45: once per batch, not
    /// twice per job).
    fn job_finished(&mut self, done: Tracked, later: bool) -> Vec<Effect> {
        let panel = done.panel;
        let snapshot = &done.snapshot;
        let completed = snapshot.state == JobState::Completed;
        if let Some(failure) = snapshot.failures.last() {
            self.pane_mut(panel).error = Some(if completed {
                t!("ops.failed_files", count = snapshot.failures.len())
            } else {
                error_text(&failure.code)
            });
        }
        let mut commands = Vec::new();
        if let (true, FollowUp::Transfer { deselect: true }) = (completed, &done.follow) {
            let names = self.copied_names(&done);
            if !names.is_empty() {
                commands.push(Command::ToggleSelection {
                    panel: Some(panel),
                    names,
                });
            }
        }
        let batch_left = later || self.file_ops.jobs.batch_followed(snapshot.batch);
        if !batch_left {
            let clean = completed && snapshot.failures.is_empty();
            commands.extend(follow_commands(panel, &done.follow, clean));
        }
        if commands.is_empty() {
            return Vec::new();
        }
        vec![Effect::Dispatch(panel, commands)]
    }

    /// The selected entries of the panel that a copy took and copied without a failure.
    fn copied_names(&self, done: &Tracked) -> Vec<String> {
        let snapshot = &done.snapshot;
        self.selected_sources(done.panel, [&snapshot.kind], &snapshot.failures)
    }

    /// The selected entries of `panel` among the sources of `kinds`, but those `failures` are
    /// in; the panel's state is read once for all of them.
    fn selected_sources<'a>(
        &self,
        panel: PanelId,
        kinds: impl IntoIterator<Item = &'a JobKind>,
        failures: &[Failure],
    ) -> Vec<String> {
        let state = self.engine.panel(panel);
        kinds
            .into_iter()
            .flat_map(jobs::sources_of)
            .filter(|source| source.path.parent() == Some(state.path.path.as_path()))
            .filter(|source| {
                !failures
                    .iter()
                    .any(|failure| failure.path.path.starts_with(&source.path))
            })
            .filter_map(|source| source.path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| state.selection.contains(name))
            .collect()
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
