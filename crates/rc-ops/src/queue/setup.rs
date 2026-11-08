//! What a job's runner is given: its backends, the queue's settings and hooks, and - for a job an
//! MCP call submitted - its read guard (`read_guard.rs`, RC-080-42).

use std::sync::Arc;

use rc_vfs::BackendId;

use super::{Backend, Hooks, JobShared, Queue, lock};
use crate::{
    JobOptions, JobSpec,
    journal::StepJournal,
    read_guard::{JobGuard, marked},
    runner::{Resumption, Setup},
};

impl Queue {
    /// Lets the jobs submitted or loaded from now on while `guards` apply (inside an MCP call)
    /// read only what they admit, also after a restart.
    pub fn set_read_guards(&self, guards: Arc<dyn crate::ReadGuards>) {
        lock(&self.inner.hooks).read_guards = Some(guards);
    }

    /// `spec` as the queue takes it now: marked `read_guarded` exactly when the read guards
    /// apply; a caller's own mark counts for nothing.
    pub(super) fn marked(&self, spec: JobSpec) -> JobSpec {
        marked(spec, self.hooks().read_guards.as_ref())
    }

    /// What the job's runner is given: its backends, the settings, its journal, how it resumes
    /// and, when it is guarded, the read guard of its source.
    pub(super) fn setup(
        &self,
        job: &Arc<JobShared>,
        options: JobOptions,
        (source, target): ((BackendId, Backend), Backend),
        journal: Option<Arc<dyn StepJournal>>,
        mut resume: Resumption,
    ) -> Setup {
        let config = self.config();
        let hooks = self.hooks();
        resume.retry = config.retry;
        resume.reconnect = hooks.reconnect;
        let read_guard = JobGuard::of(options.read_guarded, hooks.read_guards.as_ref());
        Setup {
            job: Arc::clone(job),
            options,
            buffer_size: config.buffer_size,
            progress_interval: config.progress_interval,
            src_id: source.0,
            src: source.1.vfs,
            trash: source.1.trash,
            dst: target.vfs,
            journal,
            archive: config.archive,
            resume,
            read_guard,
            archive_release: hooks.archive_release,
            ignore: hooks.ignore,
        }
    }

    /// Lets an archive update ask `hook` to let go of its archive before replacing it
    /// (RC-110-05): the engine closes the archive's reading sessions.
    pub fn set_archive_release(&self, hook: Arc<dyn crate::ArchiveRelease>) {
        lock(&self.inner.hooks).archive_release = Some(hook);
    }

    /// The panels' ignore list for the copies and moves started from now on, `None` while it is
    /// off (RC-100-92): they leave out what it hides.
    pub fn set_ignore(&self, rule: Option<Arc<dyn crate::IgnoreRule>>) {
        lock(&self.inner.hooks).ignore = rule;
    }

    /// The store, the reconnect hook and the read guards as set now.
    pub(super) fn hooks(&self) -> Hooks {
        lock(&self.inner.hooks).clone()
    }
}
