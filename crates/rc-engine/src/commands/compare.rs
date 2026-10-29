//! The `compare.*` and `sync.*` commands: two files by content, two directories as a
//! synchronization plan, the plan carried out as jobs of the operations queue, and the window's
//! diff window and synchronize-dirs dialog.
//!
//! `compare.files`, `compare.dirs`, `compare.select_differences` and `sync.apply` compare under
//! the token of their call (`dispatch/cancel.rs`): a caller that gives one can stop them. The
//! GUI runs the same work through [`Engine::diff_paths`], [`Engine::compare_paths`] and
//! [`Engine::plan_sync`] with a cancellation token of its own, and hands the plan a person edited
//! to [`Engine::sync_jobs`], which `sync.apply` uses too.

use std::collections::HashSet;

use rc_ops::{AT_ONCE, JobKind, JobSnapshot};
use rc_vfs::{Entry, VfsPath};
use tokio_util::sync::CancellationToken;

use super::{archive::refuse_archive_change, panel::resolve_path};
use crate::{
    Engine, EngineError, EngineEvent, Outcome, PanelChange, PanelId, Selection,
    compare::{self, FileCompareRequest, FileComparison, Side, TextDiff, TextOptions},
    dispatch::call_token,
    read_guard,
    settings::SELECT_FOLDERS_KEY,
    sync::{
        self, DirCompareRequest, ItemKind, PairState, SyncApplyRequest, SyncOptions, SyncPlan,
        SyncRun,
    },
};

impl Engine {
    /// `compare.files`. Under MCP, both files - given or picked from the panels - pass the read
    /// guard first: whether two files are equal tells about the contents of each.
    pub(crate) async fn compare_files(
        &self,
        request: FileCompareRequest,
    ) -> Result<Outcome, EngineError> {
        let (left, right) =
            self.files_to_compare(request.left.as_deref(), request.right.as_deref())?;
        for side in [&left, &right] {
            if let Some(guard) = read_guard::current(&side.backend) {
                guard.check(&side.path).await?;
            }
        }
        let (left_vfs, right_vfs) = (self.vfs(&left)?, self.vfs(&right)?);
        let comparison = compare::compare_files(
            Side {
                vfs: &*left_vfs,
                path: &left,
            },
            Side {
                vfs: &*right_vfs,
                path: &right,
            },
            &request.options,
            request.unified_limit,
            &call_token(),
        )
        .await?;
        Ok(Outcome::FileComparison(comparison))
    }

    /// `compare.dirs`.
    pub(crate) async fn compare_dirs(
        &self,
        request: DirCompareRequest,
    ) -> Result<Outcome, EngineError> {
        let left = self.panel_relative(PanelId::Left, request.left.as_deref());
        let right = self.panel_relative(PanelId::Right, request.right.as_deref());
        let (left_vfs, right_vfs) = (self.vfs(&left)?, self.vfs(&right)?);
        let plan = sync::plan(
            Side {
                vfs: &*left_vfs,
                path: &left,
            },
            Side {
                vfs: &*right_vfs,
                path: &right,
            },
            &request.options,
            &call_token(),
        )
        .await?;
        Ok(Outcome::DirComparison(
            plan.comparison(request.show, request.limit),
        ))
    }

    /// `compare.open_diff`: the files `compare.files` would compare, announced for the diff
    /// window.
    pub(crate) fn open_diff(&self) -> Result<Outcome, EngineError> {
        let (left, right) = self.files_to_compare(None, None)?;
        self.publish(EngineEvent::CompareFiles { left, right });
        Ok(Outcome::Panel(self.snapshot(None)))
    }

    /// `compare.select_differences` (Shift+F2, Total Commander's "Compare directories"): the
    /// files of the two panels' folders compared by name, size and time (2 s tolerance) - and
    /// with `by_content`, files of one size byte by byte - and each panel's selection replaced by
    /// its files that the other side lacks, has older, or has in another size at the same time.
    /// Folders are not compared; with `selection.include_folders` on, a folder only one side has
    /// is selected too (RC-020-08). A panel that moved on meanwhile keeps its selection.
    pub(crate) async fn select_differences(
        &self,
        by_content: bool,
    ) -> Result<Outcome, EngineError> {
        let (left, right) = {
            let state = self.state();
            (
                state.panel(PanelId::Left).path.clone(),
                state.panel(PanelId::Right).path.clone(),
            )
        };
        let options = SyncOptions {
            subdirs: false,
            by_content,
            ..SyncOptions::default()
        };
        let folders = self.setting_on(SELECT_FOLDERS_KEY).await;
        let (left_vfs, right_vfs) = (self.vfs(&left)?, self.vfs(&right)?);
        let plan = sync::plan_with_lone_dirs(
            Side {
                vfs: &*left_vfs,
                path: &left,
            },
            Side {
                vfs: &*right_vfs,
                path: &right,
            },
            &options,
            folders,
            &call_token(),
        )
        .await?;
        let (mut on_left, mut on_right) = (Vec::new(), Vec::new());
        for row in &plan.rows {
            let dir = match row.kind() {
                Some(ItemKind::File) => false,
                Some(ItemKind::Dir) => true,
                None => continue,
            };
            let name = (row.path.to_string_lossy().into_owned(), dir);
            match row.state {
                PairState::LeftOnly | PairState::LeftNewer => on_left.push(name),
                PairState::RightOnly | PairState::RightNewer => on_right.push(name),
                PairState::Different => {
                    on_left.push(name.clone());
                    on_right.push(name);
                }
                PairState::Equal | PairState::KindMismatch => {}
            }
        }
        let mut changed = Vec::new();
        {
            let mut state = self.state();
            for (panel, compared, wanted) in [
                (PanelId::Left, &left, on_left),
                (PanelId::Right, &right, on_right),
            ] {
                let shown = state.panel_mut(panel);
                if shown.path == *compared {
                    shown.selection = chosen(&shown.entries, &wanted);
                    changed.push(panel);
                }
            }
        }
        for panel in changed {
            self.publish(EngineEvent::Panel {
                panel,
                change: PanelChange::Selection,
            });
        }
        Ok(Outcome::Panel(self.snapshot(None)))
    }

    /// `sync.open_dialog`: both panels' folders, announced for the dialog.
    pub(crate) fn open_sync_dialog(&self) -> Outcome {
        let (left, right) = {
            let state = self.state();
            (
                state.panel(PanelId::Left).path.clone(),
                state.panel(PanelId::Right).path.clone(),
            )
        };
        self.publish(EngineEvent::SyncDirs { left, right });
        Outcome::Panel(self.snapshot(None))
    }

    /// `sync.apply`: the two trees compared again, the request's actions set, the jobs submitted.
    /// Comparing again means only what is there now is copied or deleted.
    pub(crate) async fn sync_apply(
        &self,
        request: SyncApplyRequest,
    ) -> Result<Outcome, EngineError> {
        let left = self.panel_relative(PanelId::Left, request.left.as_deref());
        let right = self.panel_relative(PanelId::Right, request.right.as_deref());
        let mut plan = self
            .plan_sync(&left, &right, &request.options, &call_token())
            .await?;
        plan.assign(request.proposed, &request.actions)?;
        let jobs = self.sync_jobs(&plan, &request.run).await?;
        Ok(Outcome::Jobs { jobs })
    }

    /// The line diff of two files for the diff window ([`compare::diff_files`]): `None` when
    /// either is binary or too large to compare by line.
    ///
    /// # Errors
    ///
    /// Either file cannot be read, or `cancel` was cancelled.
    pub async fn diff_paths(
        &self,
        left: &VfsPath,
        right: &VfsPath,
        options: &TextOptions,
        cancel: &CancellationToken,
    ) -> Result<Option<TextDiff>, EngineError> {
        let (left_vfs, right_vfs) = (self.vfs(left)?, self.vfs(right)?);
        compare::diff_files(
            Side {
                vfs: &*left_vfs,
                path: left,
            },
            Side {
                vfs: &*right_vfs,
                path: right,
            },
            options,
            cancel,
        )
        .await
    }

    /// The byte comparison of two files ([`compare::compare_files`], streamed): what the diff
    /// window shows for files it cannot compare by line.
    ///
    /// # Errors
    ///
    /// Either path is a directory or cannot be read, or `cancel` was cancelled.
    pub async fn compare_paths(
        &self,
        left: &VfsPath,
        right: &VfsPath,
        cancel: &CancellationToken,
    ) -> Result<FileComparison, EngineError> {
        let (left_vfs, right_vfs) = (self.vfs(left)?, self.vfs(right)?);
        compare::compare_files(
            Side {
                vfs: &*left_vfs,
                path: left,
            },
            Side {
                vfs: &*right_vfs,
                path: right,
            },
            &TextOptions::default(),
            0,
            cancel,
        )
        .await
    }

    /// Compares two trees ([`sync::plan`]).
    ///
    /// # Errors
    ///
    /// A directory cannot be read, or `cancel` was cancelled.
    pub async fn plan_sync(
        &self,
        left: &VfsPath,
        right: &VfsPath,
        options: &SyncOptions,
        cancel: &CancellationToken,
    ) -> Result<SyncPlan, EngineError> {
        let (left_vfs, right_vfs) = (self.vfs(left)?, self.vfs(right)?);
        sync::plan(
            Side {
                vfs: &*left_vfs,
                path: left,
            },
            Side {
                vfs: &*right_vfs,
                path: right,
            },
            options,
            cancel,
        )
        .await
    }

    /// Submits the jobs that carry out `plan`'s actions ([`SyncPlan::jobs`]) and answers them,
    /// in the order submitted: at once as one batch, else queued one by one
    /// ([`Engine::tool_list`]). Every job is checked before the first is submitted: nothing is
    /// written into an archive, and under MCP no copy carries a file the read guard keeps.
    ///
    /// # Errors
    ///
    /// `sync_deletes_unconfirmed`, `archive_read_only`, `sensitive_path`, or the queue refused
    /// a job (at once none runs; queued, the jobs before it run).
    pub async fn sync_jobs(
        &self,
        plan: &SyncPlan,
        run: &SyncRun,
    ) -> Result<Vec<JobSnapshot>, EngineError> {
        let specs = plan.jobs(run)?;
        for spec in &specs {
            match &spec.kind {
                JobKind::Copy { sources, target } => {
                    refuse_archive_change(target)?;
                    self.open_sources(sources).await?;
                    self.refuse_guarded(sources).await?;
                }
                JobKind::Mkdir { path } => refuse_archive_change(path)?,
                JobKind::Delete { sources, .. } => {
                    sources.iter().try_for_each(refuse_archive_change)?;
                }
                _ => {}
            }
        }
        if self.tool_list().await == AT_ONCE {
            // At once as one batch, one job after the other, so the plan's order holds: an empty
            // folder before the deletes, the deletes after the copies (RC-010-28).
            return Ok(self.inner.ops.submit_batch(specs, AT_ONCE)?);
        }
        specs
            .into_iter()
            .map(|spec| self.inner.ops.submit(spec).map_err(EngineError::from))
            .collect()
    }

    /// `target` against `panel`'s directory; the directory itself when `None`.
    fn panel_relative(&self, panel: PanelId, target: Option<&str>) -> VfsPath {
        let current = self.state().panel(panel).path.clone();
        match target {
            Some(target) => resolve_path(&current, target),
            None => current,
        }
    }

    /// The files of `compare.files`, as Total Commander picks them: given paths against their
    /// panels; without paths the two selected files of the active panel, else the file under
    /// each panel's cursor.
    fn files_to_compare(
        &self,
        left: Option<&str>,
        right: Option<&str>,
    ) -> Result<(VfsPath, VfsPath), EngineError> {
        let state = self.state();
        if left.is_none() && right.is_none() {
            let active = state.panel(state.active);
            let selected: Vec<VfsPath> = active
                .entries
                .iter()
                .filter(|entry| !entry.is_dir_like() && active.selection.contains(&entry.name))
                .map(|entry| active.path.join(&entry.name))
                .collect();
            if let [first, second] = selected.as_slice() {
                return Ok((first.clone(), second.clone()));
            }
        }
        let pick = |panel: PanelId, given: Option<&str>| {
            let shown = state.panel(panel);
            match given {
                Some(given) => Some(resolve_path(&shown.path, given)),
                None => shown
                    .cursor_entry()
                    .filter(|entry| !entry.is_dir_like())
                    .map(|entry| shown.path.join(&entry.name)),
            }
        };
        match (pick(PanelId::Left, left), pick(PanelId::Right, right)) {
            (Some(left), Some(right)) => Ok((left, right)),
            _ => Err(EngineError::CompareNeedsFiles),
        }
    }
}

/// The entries named in `wanted`, each a file or a directory (`true`) as the comparison saw it;
/// a name not found as spelled is matched ignoring case, as the comparison paired it on a
/// case-insensitive side.
fn chosen(entries: &[Entry], wanted: &[(String, bool)]) -> Selection {
    let names: HashSet<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    let exact: HashSet<(&str, bool)> = wanted
        .iter()
        .filter(|(name, _)| names.contains(name.as_str()))
        .map(|(name, dir)| (name.as_str(), *dir))
        .collect();
    let folded: HashSet<(String, bool)> = wanted
        .iter()
        .filter(|(name, _)| !names.contains(name.as_str()))
        .map(|(name, dir)| (name.to_lowercase(), *dir))
        .collect();
    entries
        .iter()
        .filter(|entry| {
            let dir = entry.is_dir_like();
            exact.contains(&(entry.name.as_str(), dir))
                || folded.contains(&(entry.name.to_lowercase(), dir))
        })
        .map(|entry| entry.name.clone())
        .collect()
}

#[cfg(test)]
#[path = "compare_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "compare_sync_tests.rs"]
mod sync_tests;
