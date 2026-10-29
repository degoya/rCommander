//! The `ops.*` commands: jobs of the operations queue (`rc-ops`), with sources and targets
//! taken from the panels the way Total Commander's F5 to F8 take them.

use rc_ops::{
    AT_ONCE, DEFAULT_LIST, DeleteMode, ErrorPolicy, JobId, JobKind, JobOptions, JobSpec, OpsError,
    QueueListId, per_item,
};
use rc_vfs::{BackendId, EntryKind, VfsPath};

use super::{archive::refuse_archive_change, panel::resolve_path};
use crate::{
    Engine, EngineError, Outcome, PanelId, PanelState,
    mcp_questions::check_mcp_pin,
    read_guard,
    settings::{ONE_JOB_PER_ITEM_KEY, TOOLS_AT_ONCE_KEY},
};

/// Paths the read guard judges in one go: a source's tree is judged in batches of this many, so a
/// large one never holds all its paths at once.
const GUARD_BATCH: usize = 4096;

/// The sources of a job from `panel`: the names given (absolute, or relative to the panel's
/// directory), else the selection, else the entry under the cursor.
pub(super) fn sources_of(
    panel: &PanelState,
    named: &[String],
) -> Result<Vec<VfsPath>, EngineError> {
    if !named.is_empty() {
        return Ok(named
            .iter()
            .map(|name| resolve_path(&panel.path, name))
            .collect());
    }
    if !panel.selection.is_empty() {
        return Ok(panel
            .selection
            .iter()
            .map(|name| panel.path.join(name))
            .collect());
    }
    panel
        .cursor_entry()
        .map(|entry| vec![panel.path.join(&entry.name)])
        .ok_or(EngineError::Ops(OpsError::NoSources))
}

impl Engine {
    /// `ops.copy` and `ops.move`: from `panel` (the active one by default) into `target`, or
    /// into the other panel's directory. A source or target may be written out as a server's
    /// location (`sftp://<connection>/<path>`, `ftp://...`); a server's backend is opened, or
    /// opened again after its session went, before the job is submitted (RC-060-05). Under MCP
    /// the sources pass the read guard first, and a panel the person was asked about must still
    /// show the folder the question showed (`panel_changed`, RC-080-34).
    pub(crate) async fn ops_transfer(
        &self,
        panel: Option<PanelId>,
        sources: &[String],
        target: Option<&str>,
        options: JobOptions,
        moving: bool,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        self.remember_selection(id, sources);
        let other = {
            let state = self.state();
            let other = &state.panel(id.other()).path;
            check_mcp_pin(id.other(), other)?;
            other.clone()
        };
        let sources = self.located_sources(id, sources).await?;
        let target = match target {
            Some(path) => self.locate(&other, path).await?,
            None => other,
        };
        // Archives are read-only: nothing goes into one, nothing moves out of one.
        refuse_archive_change(&target)?;
        if moving {
            sources.iter().try_for_each(refuse_archive_change)?;
        }
        self.open_sources(&sources).await?;
        self.open_server(&target).await?;
        self.refuse_guarded(&sources).await?;
        self.apply_transfer_settings().await;
        let kind = if moving {
            JobKind::Move { sources, target }
        } else {
            JobKind::Copy { sources, target }
        };
        self.submit_items(JobSpec { kind, options }, DEFAULT_LIST)
            .await
    }

    /// Submits `spec` into the queue list `list`: with `queue.one_job_per_item` on (the default)
    /// a copy, move or delete of several sources as one job per source, side by side in their
    /// order ([`per_item`], RC-010-14), the outcome listing them; else one job.
    pub(super) async fn submit_items(
        &self,
        spec: JobSpec,
        list: QueueListId,
    ) -> Result<Outcome, EngineError> {
        let specs = if self.setting_on(ONE_JOB_PER_ITEM_KEY).await {
            per_item(spec)
        } else {
            vec![spec]
        };
        let mut jobs = self.inner.ops.submit_batch(specs, list)?;
        Ok(if jobs.len() == 1 {
            Outcome::job(jobs.remove(0))
        } else {
            Outcome::Jobs { jobs }
        })
    }

    /// The list a job of packing, unpacking, checksums, synchronizing or multi-rename goes into:
    /// [`AT_ONCE`] with `queue.tools_at_once` on (the default), so it never waits in a lane
    /// behind a copy (RC-010-28); else the default list, as before.
    pub(super) async fn tool_list(&self) -> QueueListId {
        if self.setting_on(TOOLS_AT_ONCE_KEY).await {
            AT_ONCE
        } else {
            DEFAULT_LIST
        }
    }

    /// The sources of a job from panel `id` as `ops.copy` takes them: each name located
    /// ([`Engine::locate`]: relative to the panel's directory, absolute, or a server's location
    /// `sftp://<connection>/<path>` or `ftp://...`), else the selection, else the entry under
    /// the cursor. A panel an MCP call was asked about is read only in the folder the question
    /// showed.
    async fn located_sources(
        &self,
        id: PanelId,
        named: &[String],
    ) -> Result<Vec<VfsPath>, EngineError> {
        let current = {
            let state = self.state();
            let panel = state.panel(id);
            check_mcp_pin(id, &panel.path)?;
            if named.is_empty() {
                return sources_of(panel, named);
            }
            panel.path.clone()
        };
        let mut located = Vec::with_capacity(named.len());
        for name in named {
            located.push(self.locate(&current, name).await?);
        }
        Ok(located)
    }

    /// Opens the backend of every source for the queue, each one once: the sources of one call
    /// may lie on several - a local file and one at a server's location (RC-080-50). With one
    /// job per item each source's job reads its own; one job of mixed sources the queue refuses
    /// before it runs (`mixed_sources`).
    pub(super) async fn open_sources(&self, sources: &[VfsPath]) -> Result<(), EngineError> {
        let mut opened: Vec<&BackendId> = Vec::new();
        for source in sources {
            if opened.contains(&&source.backend) {
                continue;
            }
            opened.push(&source.backend);
            self.open_backend(source)?;
            self.open_server(source).await?;
        }
        Ok(())
    }

    /// Opens the SFTP or FTP backend of `path`'s connection for the queue, when it is one.
    pub(super) async fn open_server(&self, path: &VfsPath) -> Result<(), EngineError> {
        if matches!(path.backend, BackendId::Sftp { .. } | BackendId::Ftp { .. }) {
            self.open_vfs(path, self.announces_locked_vault()).await?;
        }
        Ok(())
    }

    /// Under MCP, refuses sources that hold - themselves or anywhere below them - a file the
    /// read guard keeps from MCP: a copy would carry its contents to a place or a name the guard
    /// no longer knows (a folder outside `.ssh`, a conflict's `name (2)`). Every entry is judged,
    /// a link by where it leads; links are not entered, and a folder that cannot be listed is
    /// one the job cannot read either. The walk gathers the paths and the guard judges them in
    /// one go off the async threads, [`GUARD_BATCH`] at a time
    /// ([`read_guard::ReadGuard::check_all`]).
    pub(super) async fn refuse_guarded(&self, sources: &[VfsPath]) -> Result<(), EngineError> {
        for source in sources {
            let Some(guard) = read_guard::current(&source.backend) else {
                continue;
            };
            let vfs = self.vfs(source)?;
            let mut batch = vec![source.path.clone()];
            let mut dirs = Vec::new();
            if vfs
                .stat(&source.path)
                .await
                .is_ok_and(|entry| entry.kind == EntryKind::Dir)
            {
                dirs.push(source.path.clone());
            }
            while let Some(dir) = dirs.pop() {
                let Ok(entries) = vfs.list(&dir).await else {
                    continue;
                };
                for entry in entries {
                    let path = dir.join(&entry.name);
                    if entry.kind == EntryKind::Dir {
                        dirs.push(path.clone());
                    }
                    batch.push(path);
                    if batch.len() >= GUARD_BATCH {
                        guard.check_all(std::mem::take(&mut batch)).await?;
                    }
                }
            }
            guard.check_all(batch).await?;
        }
        Ok(())
    }

    /// `ops.delete`: the sources as `ops.copy` takes them, a server's location among them
    /// (RC-060-05); its backend is opened for the queue first. Into the queue list `queue` (F2),
    /// else at once, outside the lists ([`AT_ONCE`]).
    pub(crate) async fn ops_delete(
        &self,
        panel: Option<PanelId>,
        sources: &[String],
        (mode, on_error): (DeleteMode, ErrorPolicy),
        queue: Option<QueueListId>,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        self.remember_selection(id, sources);
        let sources = self.located_sources(id, sources).await?;
        sources.iter().try_for_each(refuse_archive_change)?;
        self.open_sources(&sources).await?;
        let spec = JobSpec {
            kind: JobKind::Delete { sources, mode },
            options: JobOptions {
                on_error,
                ..JobOptions::default()
            },
        };
        self.submit_items(spec, queue.unwrap_or(AT_ONCE)).await
    }

    /// `ops.mkdir`: `path` resolved against the panel's directory, or a server's location as
    /// `ops.copy` takes it (RC-060-05), whose backend is opened for the queue first. At once
    /// ([`AT_ONCE`]), never in the drive's lane behind a copy.
    pub(crate) async fn ops_mkdir(
        &self,
        panel: Option<PanelId>,
        path: &str,
    ) -> Result<Outcome, EngineError> {
        let path = self.locate_from(panel, path).await?;
        refuse_archive_change(&path)?;
        self.open_server(&path).await?;
        let spec = JobSpec {
            kind: JobKind::Mkdir { path },
            options: JobOptions::default(),
        };
        Ok(Outcome::job(
            self.inner.ops.submit_into(spec, None, AT_ONCE)?,
        ))
    }

    /// `ops.list`: one job, or all.
    pub(crate) fn ops_list(&self, job: Option<JobId>) -> Result<Outcome, EngineError> {
        let jobs = match job {
            Some(id) => vec![self.inner.ops.job(id)?],
            None => self.inner.ops.jobs(),
        };
        Ok(Outcome::Jobs { jobs })
    }
}

#[cfg(test)]
#[path = "ops_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "ops_tools_tests.rs"]
mod tools_tests;
