//! The `multi_rename.*` commands: Total Commander's multi-rename tool over any `Vfs`.
//!
//! `preview` plans a rule against a fresh listing of the folder (`rc_rename::plan`). `apply`
//! plans again - it never takes a plan from outside - and refuses one with conflicts, writes
//! every step of the schedule into the undo log (`rc-db`, `rename_log`) and queues the steps as
//! one rename job of `rc-ops`, whose journal marks each step in the log once it is carried out.
//! `undo` reverses the latest run as far as it got: the marks, corrected against the listing
//! after a crash (`rc_rename::applied_count`), checked against the folder as it is now
//! (`check_undo`) and refused as a whole when an entry no longer fits; the undo job clears the
//! marks again step by step, so an undo cancelled half way can be continued. One run at a time,
//! claimed atomically (`rename_busy`).
//!
//! A run that fails while entries are under temporary names is reversed by its own job with
//! `rename.undo_on_failure` on (the default, RC-080-64): the undo's steps, each mark cleared as
//! the undo job clears it, and the job's report says whether every name is as before or which
//! entries are still parked.
//!
//! A new name with a subfolder (`sub/name`, RC-020-02) is held against what that subfolder holds
//! (each one listed once, at most [`MAX_NESTED_DIRS`]), and an undo looks the nested names of
//! its run up one by one, since the folder's listing does not show them. A run that went through
//! leaves its rule as the one last used (`multi_rename_presets.rs`).
//!
//! The request brings what only the window knows (RC-100-96): the clipboard's text for `[X]`, the
//! moments of `[T2]` and `[T3]` - the time of the call where it gives none - and the local date
//! and time formats. The undo and the next step are `multi_rename_undo.rs`.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use rc_db::{Db, DbError};
use rc_ops::{ErrorPolicy, JobKind, JobOptions, JobSpec, OpsError, StepJournal};
use rc_rename::{
    Change, FsRules, PlanInput, RenameContext, RenameError, RenamePlan, RenameRule, RenameSource,
    RenameStep, SUBFOLDER_SEPARATOR, nested_path,
};
use rc_vfs::{BackendId, Capabilities, Vfs, VfsPath};
use serde::{Deserialize, Serialize};

use super::panel::resolve_path;
use crate::{
    Engine, EngineError, Outcome, PanelId, PanelState, settings::RENAME_UNDO_ON_FAILURE_KEY,
};

/// What `multi_rename.preview` and `multi_rename.apply` take.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MultiRenameRequest {
    /// The panel `dir` starts from and whose selection is renamed; the active one when `None`.
    pub panel: Option<PanelId>,
    /// The folder: absolute, or relative to the panel's; the panel's own when `None`.
    pub dir: Option<String>,
    /// Entries of the folder, in the order the counter counts them. Empty: in the panel's own
    /// folder its selection in panel order, else the entry under its cursor.
    pub names: Vec<String>,
    /// Masks, case, search and replace, counter - or a list of names.
    pub rule: RenameRule,
    /// The clipboard's text, the dialog's moments and the local formats the placeholders read;
    /// a moment not given is the time of the call.
    pub context: RenameContext,
}

/// The claim on starting a multi-rename run ([`Engine::claim_rename`]); released when dropped,
/// on every way out of the command.
pub(super) struct RenameClaim<'a>(&'a AtomicBool);

impl Drop for RenameClaim<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// A plan and the folder it renames in.
struct Planned {
    dir: VfsPath,
    plan: RenamePlan,
}

/// The naming rules of `dir`'s file system: the platform's for local folders, Unix lengths for
/// the others, upper and lower case as the backend reports it.
pub(super) fn fs_rules(dir: &VfsPath, capabilities: Capabilities) -> FsRules {
    let base = if dir.backend == BackendId::Local {
        FsRules::native()
    } else {
        FsRules::UNIX
    };
    FsRules {
        case_insensitive: !capabilities.case_sensitive,
        ..base
    }
}

/// The selected entries of `panel` in its order, else the entry under its cursor.
fn panel_names(panel: &PanelState) -> Vec<String> {
    if panel.selection.is_empty() {
        return panel
            .cursor_entry()
            .map(|entry| vec![entry.name.clone()])
            .unwrap_or_default();
    }
    panel
        .entries
        .iter()
        .filter(|entry| panel.selection.contains(&entry.name))
        .map(|entry| entry.name.clone())
        .collect()
}

/// The most subfolders of a plan's nested new names that are listed for names taken there;
/// beyond them a taken name stops the run's step instead (`rc-ops` never overwrites).
const MAX_NESTED_DIRS: usize = 64;

/// The nested names (`sub/name`) the subfolders of `plan`'s new names hold already. Only items
/// without a conflict count: their paths stay below `dir` (`rc_rename::check_path`). A subfolder
/// that cannot be listed - missing, most often - holds nothing yet.
async fn nested_entries(vfs: &dyn Vfs, dir: &Path, plan: &RenamePlan) -> Vec<String> {
    let mut folders: Vec<&str> = Vec::new();
    for item in &plan.items {
        if item.change == Change::Unchanged || item.conflict.is_some() {
            continue;
        }
        let Some((folder, _)) = item.new.rsplit_once(SUBFOLDER_SEPARATOR) else {
            continue;
        };
        if !folders.contains(&folder) {
            if folders.len() == MAX_NESTED_DIRS {
                break;
            }
            folders.push(folder);
        }
    }
    let mut names = Vec::new();
    for folder in folders {
        if let Ok(entries) = vfs.list(&nested_path(dir, folder)).await {
            names.extend(
                entries
                    .into_iter()
                    .map(|entry| format!("{folder}{SUBFOLDER_SEPARATOR}{}", entry.name)),
            );
        }
    }
    names
}

/// The plan of `request`'s rule for `sources` in `dir`.
fn plan_in(
    dir: &str,
    sources: &[RenameSource],
    existing: &[String],
    fs: FsRules,
    request: (&RenameRule, &RenameContext),
) -> Result<RenamePlan, RenameError> {
    let (rule, context) = request;
    let input = PlanInput {
        dir,
        sources,
        existing,
        fs,
        context,
    };
    rc_rename::plan(&input, rule)
}

/// `context` with the moments it leaves open at `now`: what `[T2]` and `[T3]` read for an agent.
fn with_moments(context: &RenameContext, now: SystemTime) -> RenameContext {
    let now = rc_rename::local_time(now);
    let mut context = context.clone();
    context.opened_at.get_or_insert(now);
    context.changed_at.get_or_insert(now);
    context
}

/// A new run's tag, also the middle of its temporary names: the time in microseconds, in hex.
fn run_tag() -> String {
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_micros());
    format!("{micros:x}")
}

/// How the log names a run's folder: the `VfsPath` as JSON.
fn dir_key(dir: &VfsPath) -> Result<String, EngineError> {
    serde_json::to_string(dir).map_err(|source| {
        EngineError::Database(DbError::Json {
            key: "rename_log.dir".to_owned(),
            source,
        })
    })
}

/// Which way a rename job moves the marks of its run.
#[derive(Debug, Clone, Copy)]
pub(super) enum Direction {
    /// Step `index` of the run is carried out.
    Apply,
    /// The undo's step `index` reversed the run's step `applied - 1 - index`.
    Undo { applied: usize },
}

/// A rename job's journal: the marks of its run in the undo log.
pub(super) struct Journal {
    pub(super) db: Db,
    pub(super) run: String,
    pub(super) direction: Direction,
}

#[async_trait]
impl StepJournal for Journal {
    async fn done(&self, index: usize) -> Result<(), String> {
        let (seq, done) = match self.direction {
            Direction::Apply => (index, true),
            Direction::Undo { applied } => (applied.saturating_sub(index + 1), false),
        };
        self.db
            .mark_rename_step(&self.run, seq, done)
            .await
            .map_err(|error| error.to_string())
    }

    /// The run rolled back by its own job (RC-080-64): only an apply's journal hears it.
    async fn undone(&self, index: usize) -> Result<(), String> {
        self.db
            .mark_rename_step(&self.run, index, false)
            .await
            .map_err(|error| error.to_string())
    }
}

impl Engine {
    /// `multi_rename.preview`: the plan, nothing renamed.
    pub(crate) async fn multi_rename_preview(
        &self,
        request: &MultiRenameRequest,
    ) -> Result<Outcome, EngineError> {
        let Planned { dir, plan } = self.plan_rename(request).await?;
        Ok(Outcome::RenamePreview { dir, plan })
    }

    /// `multi_rename.apply`: the plan made anew, logged and queued.
    pub(crate) async fn multi_rename_apply(
        &self,
        request: &MultiRenameRequest,
    ) -> Result<Outcome, EngineError> {
        let db = self.db()?.clone();
        let _claim = self.claim_rename()?;
        let Planned { dir, plan } = self.plan_rename(request).await?;
        let last = db.clone();
        let run = run_tag();
        // Refuses a plan with conflicts before anything is written.
        let schedule = rc_rename::schedule(&plan, &run)?;
        if schedule.steps.is_empty() {
            return Err(EngineError::RenameNoChanges);
        }
        let pairs: Vec<(&str, &str)> = schedule
            .steps
            .iter()
            .map(|step| (step.from.as_str(), step.to.as_str()))
            .collect();
        db.log_rename_run(&run, &dir_key(&dir)?, &pairs).await?;
        let journal = Journal {
            db,
            run: run.clone(),
            direction: Direction::Apply,
        };
        let rollback = self.setting_on(RENAME_UNDO_ON_FAILURE_KEY).await;
        let outcome = self
            .submit_rename(run, dir, schedule.steps, (journal, rollback))
            .await?;
        // Offered again as "last used" - without a name list, which belongs to these entries
        // only; a rule that cannot be stored is only missing there.
        let rule = RenameRule {
            names: None,
            ..request.rule.clone()
        };
        if let Err(error) = last
            .set(super::multi_rename_presets::LAST_RULE_KEY, &rule)
            .await
        {
            tracing::warn!(%error, "the last multi-rename rule was not stored");
        }
        Ok(outcome)
    }

    /// The plan of `request` against a fresh listing of its folder.
    async fn plan_rename(&self, request: &MultiRenameRequest) -> Result<Planned, EngineError> {
        let id = self.resolve(request.panel);
        let (dir, wanted) = {
            let state = self.state();
            let panel = state.panel(id);
            match &request.dir {
                Some(dir) => (resolve_path(&panel.path, dir), request.names.clone()),
                None if request.names.is_empty() => (panel.path.clone(), panel_names(panel)),
                None => (panel.path.clone(), request.names.clone()),
            }
        };
        if wanted.is_empty() {
            return Err(EngineError::Ops(OpsError::NoSources));
        }
        let vfs = self.vfs(&dir)?;
        let entries = vfs.list(&dir.path).await?;
        let by_name: HashMap<&str, &rc_vfs::Entry> = entries
            .iter()
            .map(|entry| (entry.name.as_str(), entry))
            .collect();
        let mut seen = HashSet::new();
        let mut sources = Vec::with_capacity(wanted.len());
        for name in &wanted {
            if !seen.insert(name.as_str()) {
                continue;
            }
            let entry = by_name
                .get(name.as_str())
                .ok_or_else(|| EngineError::EntryNotFound { name: name.clone() })?;
            sources.push(RenameSource {
                name: entry.name.clone(),
                modified: entry.modified.map(rc_rename::local_time),
                size: Some(entry.size),
                is_dir: entry.is_dir_like(),
            });
        }
        let mut existing: Vec<String> = entries.iter().map(|entry| entry.name.clone()).collect();
        let dir_text = dir.path.to_string_lossy();
        let fs = fs_rules(&dir, vfs.capabilities());
        let context = with_moments(&request.context, SystemTime::now());
        let rule = (&request.rule, &context);
        let mut plan = plan_in(&dir_text, &sources, &existing, fs, rule)?;
        let nested = nested_entries(vfs.as_ref(), &dir.path, &plan).await;
        if !nested.is_empty() {
            existing.extend(nested);
            plan = plan_in(&dir_text, &sources, &existing, fs, rule)?;
        }
        Ok(Planned { dir, plan })
    }

    /// The one claim on starting a run, or `rename_busy`: refused while another apply or undo
    /// holds it, or while a rename job is queued or running - its plan would be made from a
    /// folder that is changing, and an undo would race the run it reverses. The claim is taken
    /// atomically and held until the caller's job is in the queue, where the job's own state
    /// takes over (set before its end is announced), so two calls at the same moment never both
    /// start.
    pub(super) fn claim_rename(&self) -> Result<RenameClaim<'_>, EngineError> {
        let flag = &self.inner.rename_claim;
        if flag
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(EngineError::RenameBusy);
        }
        let claim = RenameClaim(flag);
        let running = self
            .inner
            .ops
            .jobs()
            .iter()
            .any(|job| matches!(job.kind, JobKind::Rename { .. }) && !job.state.is_finished());
        if running {
            return Err(EngineError::RenameBusy);
        }
        Ok(claim)
    }

    /// Queues `steps` in `dir` as one rename job journaled into `run`. A failing step stops it:
    /// the steps after it rely on it; with `rollback` the job reverses its steps done when that
    /// leaves entries under temporary names. At once or queued ([`Engine::tool_list`]).
    pub(super) async fn submit_rename(
        &self,
        run: String,
        dir: VfsPath,
        steps: Vec<RenameStep>,
        (journal, rollback): (Journal, bool),
    ) -> Result<Outcome, EngineError> {
        let spec = JobSpec {
            kind: JobKind::rename(dir.clone(), steps).rolling_back(rollback),
            options: JobOptions {
                on_error: ErrorPolicy::Cancel,
                ..JobOptions::default()
            },
        };
        let journal: Arc<dyn StepJournal> = Arc::new(journal);
        let list = self.tool_list().await;
        let job = self.inner.ops.submit_into(spec, Some(journal), list)?;
        Ok(Outcome::RenameRun {
            run,
            dir,
            job: Box::new(job),
        })
    }
}

#[cfg(test)]
#[path = "multi_rename_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "multi_rename_rollback_tests.rs"]
mod rollback_tests;

#[cfg(test)]
#[path = "multi_rename_context_tests.rs"]
mod context_tests;
