//! The jobs the window started, as the queue reports them: the one in the progress window, the
//! ones sent into the background, the question one of them asks.
//!
//! A job is adopted when the engine accepted it. Its events may arrive before that; the window
//! then takes the job's snapshot from the queue, so nothing it asked in the meantime is lost.
//! Jobs of other callers (an agent's) are not followed: they answer their questions in advance.
//!
//! The progress window's total covers the whole operation, as in Total Commander (RC-010-07):
//! every job of the batch, the finished ones remembered here until the last one is over, with
//! its speed, time left and time since the start measured over the batch (`pace.rs`).

use std::{collections::BTreeSet, time::Instant};

use rc_engine::{JobChange, JobId, JobKind, JobSnapshot, JobState, PanelId, Question};

pub use super::totals::{Timing, Totals};
use super::{
    dialog::FollowUp,
    pace::{self, Pace},
    totals::ratio,
};

/// One job of the window.
#[derive(Debug, Clone)]
pub struct Tracked {
    pub panel: PanelId,
    pub follow: FollowUp,
    /// In the progress window; otherwise in the background bar.
    pub foreground: bool,
    pub snapshot: JobSnapshot,
    /// "Skip all" was answered to a failing file: later ones are skipped without asking.
    pub skip_errors: bool,
    /// The name "rename to" offers in the conflict dialog, as edited.
    pub rename_to: String,
    /// Why "rename to" did nothing, for the conflict dialog to show.
    pub note: Option<&'static str>,
}

impl Tracked {
    pub fn id(&self) -> JobId {
        self.snapshot.id
    }

    /// The share done, from 0 to 1 ([`share_of`]); the bar reads it through the queue's entries.
    #[cfg(test)]
    pub fn share(&self) -> f32 {
        share_of(&self.snapshot)
    }

    /// The share of the file at work, from 0 to 1, by its bytes.
    pub fn file_share(&self) -> f32 {
        let progress = &self.snapshot.progress;
        ratio(progress.file_bytes_done, progress.file_bytes_total)
    }

    pub fn paused(&self) -> bool {
        self.snapshot.state == JobState::Paused
    }

    fn operation(&self) -> Operation {
        match self.snapshot.batch {
            Some(batch) => Operation::Batch(batch),
            None => Operation::Job(self.id()),
        }
    }
}

/// One operation: the jobs of a batch, or a job of none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operation {
    Batch(u64),
    Job(JobId),
}

/// The window's jobs, oldest first.
#[derive(Debug, Default)]
pub struct Jobs {
    list: Vec<Tracked>,
    /// What the finished jobs of each batch did, while a job of it is followed.
    finished: Vec<(u64, Totals)>,
    /// How fast each operation goes, from its start until its last job is over.
    paces: Vec<(Operation, Pace)>,
}

impl Jobs {
    /// Follows `snapshot`; a finished job is not kept and comes back at once. The window adopts
    /// the jobs of a submission together ([`Jobs::adopt_unmeasured`]).
    #[cfg(test)]
    pub fn adopt(
        &mut self,
        snapshot: JobSnapshot,
        panel: PanelId,
        follow: FollowUp,
        foreground: bool,
    ) -> Option<Tracked> {
        let id = snapshot.id;
        let done = self.adopt_unmeasured(snapshot, panel, follow, foreground);
        if done.is_none() {
            self.measure(id, Instant::now(), false);
        }
        done
    }

    /// [`Jobs::adopt`] without sampling the operation's pace: the jobs of one submission are
    /// sampled once together ([`Jobs::measure_adopted`], RC-080-45).
    pub fn adopt_unmeasured(
        &mut self,
        snapshot: JobSnapshot,
        panel: PanelId,
        follow: FollowUp,
        foreground: bool,
    ) -> Option<Tracked> {
        let rename_to = proposal(snapshot.question.as_ref());
        let tracked = Tracked {
            panel,
            follow,
            foreground,
            snapshot,
            skip_errors: false,
            rename_to,
            note: None,
        };
        if tracked.snapshot.state.is_finished() {
            self.remember(&tracked.snapshot);
            return Some(tracked);
        }
        self.list.push(tracked);
        None
    }

    /// Samples the pace of each operation among the jobs `ids` just adopted, once, through its
    /// first job that is not queued - one pass over the jobs, however many a batch has.
    pub fn measure_adopted(&mut self, ids: &[JobId]) {
        let ids: BTreeSet<JobId> = ids.iter().copied().collect();
        let mut firsts: Vec<(Operation, JobId)> = Vec::new();
        for job in &self.list {
            if !ids.contains(&job.id()) || job.snapshot.state == JobState::Queued {
                continue;
            }
            let operation = job.operation();
            if !firsts.iter().any(|(of, _)| *of == operation) {
                firsts.push((operation, job.id()));
            }
        }
        let now = Instant::now();
        for (_, id) in firsts {
            self.measure(id, now, false);
        }
    }

    /// Whether a job of `batch` is still followed; never for a job of no batch.
    pub fn batch_followed(&self, batch: Option<u64>) -> bool {
        batch.is_some() && self.list.iter().any(|job| job.snapshot.batch == batch)
    }

    /// The total the progress window shows for job `id`: over its batch, the finished jobs
    /// included, or of the job alone when it is no batch's.
    pub fn totals(&self, id: JobId) -> Totals {
        let mut totals = Totals::default();
        let Some(job) = self.get(id) else {
            return totals;
        };
        let Some(batch) = job.snapshot.batch else {
            totals.add(&job.snapshot);
            return totals;
        };
        if let Some((_, done)) = self.finished.iter().find(|(of, _)| *of == batch) {
            totals = *done;
        }
        for sibling in self
            .list
            .iter()
            .filter(|job| job.snapshot.batch == Some(batch))
        {
            totals.add(&sibling.snapshot);
        }
        totals
    }

    /// Keeps what a finished job of a batch did for the batch's total.
    fn remember(&mut self, snapshot: &JobSnapshot) {
        let Some(batch) = snapshot.batch else {
            return;
        };
        match self.finished.iter_mut().find(|(of, _)| *of == batch) {
            Some((_, totals)) => totals.add_finished(snapshot),
            None => {
                let mut totals = Totals::default();
                totals.add_finished(snapshot);
                self.finished.push((batch, totals));
            }
        }
    }

    /// Forgets the operations none of whose jobs is followed any longer.
    fn forget_done(&mut self) {
        let list = &self.list;
        self.finished
            .retain(|(batch, _)| list.iter().any(|job| job.snapshot.batch == Some(*batch)));
        self.paces
            .retain(|(of, _)| list.iter().any(|job| job.operation() == *of));
    }

    /// Samples the operation of job `id` at `now` - from the first job that started, through
    /// every job of its batch; `reset` after a pause or a question.
    fn measure(&mut self, id: JobId, now: Instant, reset: bool) {
        let Some(job) = self
            .get(id)
            .filter(|job| job.snapshot.state != JobState::Queued)
        else {
            return;
        };
        let operation = job.operation();
        let totals = self.totals(id);
        let at = match self.paces.iter().position(|(of, _)| *of == operation) {
            Some(at) => at,
            None => {
                self.paces.push((operation, Pace::new(now)));
                self.paces.len() - 1
            }
        };
        let pace = &mut self.paces[at].1;
        if reset {
            pace.reset();
        }
        pace.sample(now, totals.bytes_done, totals.files_done);
    }

    /// Speed, time left and time since the start of the operation of job `id`, at `now`: over
    /// its batch, the time left the longer of bytes and files left at their rates; a job of no
    /// batch keeps its own speed and time left.
    pub fn timing(&self, id: JobId, now: Instant) -> Timing {
        let Some(job) = self.get(id) else {
            return Timing::default();
        };
        let operation = job.operation();
        let pace = self
            .paces
            .iter()
            .find(|(of, _)| *of == operation)
            .map(|(_, pace)| pace);
        let elapsed_seconds = pace.map(|pace| pace.elapsed(now));
        if job.snapshot.batch.is_none() {
            let progress = &job.snapshot.progress;
            return Timing {
                bytes_per_second: progress.bytes_per_second,
                eta_seconds: progress.eta_seconds,
                estimate: false,
                elapsed_seconds,
            };
        }
        let totals = self.totals(id);
        let (bytes, files) = totals.left();
        let rates = pace.and_then(Pace::rates);
        Timing {
            bytes_per_second: rates.map_or(0, |rates| rates.bytes as u64),
            eta_seconds: rates.and_then(|rates| pace::time_left(bytes, files, rates)),
            estimate: totals.is_estimate(),
            elapsed_seconds,
        }
    }

    pub fn get(&self, id: JobId) -> Option<&Tracked> {
        self.list.iter().find(|job| job.id() == id)
    }

    pub fn get_mut(&mut self, id: JobId) -> Option<&mut Tracked> {
        self.list.iter_mut().find(|job| job.id() == id)
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// The job in the progress window.
    pub fn foreground(&self) -> Option<&Tracked> {
        self.list.iter().find(|job| job.foreground)
    }

    /// The jobs the window follows outside the progress window; the background bar shows the
    /// queue's own (`strip.rs`).
    #[cfg(test)]
    pub fn background(&self) -> impl Iterator<Item = &Tracked> {
        self.list.iter().filter(|job| !job.foreground)
    }

    /// The first job waiting on a question, in the progress window or not; an archive's
    /// password is the archive dialogs' question (`archive_ops`), not this window's.
    pub fn asking(&self) -> Option<&Tracked> {
        self.list.iter().find(|job| {
            job.snapshot
                .question
                .as_ref()
                .is_some_and(|question| !matches!(question, Question::Password { .. }))
        })
    }

    /// Puts job `id` first, so its question is the one [`Jobs::asking`] shows: "Answer" in the
    /// queue window.
    pub fn ask_first(&mut self, id: JobId) {
        if let Some(at) = self.list.iter().position(|job| job.id() == id) {
            let job = self.list.remove(at);
            self.list.insert(0, job);
        }
    }

    /// Applies `change` of job `id`; the job when this ended it. A job the window does not
    /// follow is ignored.
    pub fn apply(&mut self, id: JobId, change: JobChange) -> Option<Tracked> {
        self.apply_at(id, change, Instant::now())
    }

    /// [`Jobs::apply`] at `now`, for the operation's speed.
    pub fn apply_at(&mut self, id: JobId, change: JobChange, now: Instant) -> Option<Tracked> {
        // A pause or a question must not lower the speed after it, as in `rc_ops`.
        let reset = matches!(change, JobChange::Paused | JobChange::Answered);
        let job = self.get_mut(id)?;
        let snapshot = &mut job.snapshot;
        match change {
            JobChange::Queued => snapshot.state = JobState::Queued,
            JobChange::Started => snapshot.state = JobState::Counting,
            JobChange::Progress { progress } => {
                if matches!(snapshot.state, JobState::Queued | JobState::Counting)
                    && progress.files_total > 0
                {
                    snapshot.state = JobState::Running;
                }
                snapshot.progress = progress;
            }
            JobChange::Question { question } => {
                job.rename_to = proposal(Some(&question));
                job.note = None;
                snapshot.question = Some(question);
                snapshot.state = JobState::Waiting;
            }
            JobChange::Answered => {
                snapshot.question = None;
                snapshot.state = JobState::Running;
            }
            JobChange::Paused => snapshot.state = JobState::Paused,
            JobChange::Resumed => snapshot.state = JobState::Running,
            JobChange::Finished { state } => {
                snapshot.state = state;
                snapshot.question = None;
                let at = self.list.iter().position(|job| job.id() == id)?;
                let done = self.list.remove(at);
                self.remember(&done.snapshot);
                self.forget_done();
                self.follow_on(&done);
                return Some(done);
            }
        }
        self.measure(id, now, reset);
        None
    }

    /// The progress window passes from a finished job to the next job of its batch, so one
    /// operation of several sources stays in the window until its last job is over (RC-010-14).
    fn follow_on(&mut self, done: &Tracked) {
        let Some(batch) = done.snapshot.batch.filter(|_| done.foreground) else {
            return;
        };
        if let Some(next) = self
            .list
            .iter_mut()
            .find(|job| job.snapshot.batch == Some(batch))
        {
            next.foreground = true;
        }
    }

    /// The unfinished jobs of the batch of job `id`, in their order; `id` alone when it is no
    /// batch's.
    pub fn siblings(&self, id: JobId) -> Vec<JobId> {
        match self.get(id).and_then(|job| job.snapshot.batch) {
            Some(batch) => self
                .list
                .iter()
                .filter(|job| job.snapshot.batch == Some(batch))
                .map(Tracked::id)
                .collect(),
            None => vec![id],
        }
    }

    /// "Skip all" answered to a failing file of job `id`: later failing files of every job of its
    /// batch are skipped without asking, as they were inside one job.
    pub fn skip_errors(&mut self, id: JobId) {
        for sibling in self.siblings(id) {
            if let Some(job) = self.get_mut(sibling) {
                job.skip_errors = true;
            }
        }
    }

    /// Replaces the snapshot of job `id` with the queue's own; the job when it is over.
    #[cfg(test)]
    pub fn refresh(&mut self, snapshot: JobSnapshot) -> Option<Tracked> {
        let id = snapshot.id;
        let job = self.get_mut(id)?;
        if job.snapshot.question != snapshot.question {
            job.rename_to = proposal(snapshot.question.as_ref());
        }
        job.snapshot = snapshot;
        if job.snapshot.state.is_finished() {
            let at = self.list.iter().position(|job| job.id() == id)?;
            return Some(self.list.remove(at));
        }
        None
    }

    /// Moves job `id` into the progress window (`true`) or the background bar; only one job is
    /// in the window.
    pub fn set_foreground(&mut self, id: JobId, foreground: bool) {
        for job in &mut self.list {
            if job.id() == id {
                job.foreground = foreground;
            } else if foreground {
                job.foreground = false;
            }
        }
    }
}

/// What "rename to" offers for a conflict: the source's name with a number, `name (2).ext`.
fn proposal(question: Option<&Question>) -> String {
    let Some(Question::Conflict { source, .. }) = question else {
        return String::new();
    };
    let name = source
        .path
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    match name.rfind('.') {
        Some(at) if at > 0 => format!("{} (2){}", &name[..at], &name[at..]),
        _ => format!("{name} (2)"),
    }
}

/// The share of `job` done, from 0 to 1: by bytes, or by files where there are no bytes.
pub fn share_of(job: &JobSnapshot) -> f32 {
    let progress = &job.progress;
    if progress.bytes_total > 0 {
        ratio(progress.bytes_done, progress.bytes_total)
    } else {
        ratio(progress.files_done, progress.files_total)
    }
}

/// The sources of a copy or move, for deselecting what made it.
pub fn sources_of(kind: &JobKind) -> Vec<&rc_engine::VfsPath> {
    match kind {
        JobKind::Copy { sources, .. } | JobKind::Move { sources, .. } => sources.iter().collect(),
        JobKind::CopyAs { source, .. } | JobKind::MoveAs { source, .. } => vec![source],
        _ => Vec::new(),
    }
}

#[cfg(test)]
#[path = "jobs_tests.rs"]
mod tests;
