//! The panels' git marks kept up to date (RC-100-97).
//!
//! A panel that shows a folder anew - navigated, read again (F2, Ctrl+R, after an operation, on
//! the window's return), a tab - asks for its marks ([`Engine::git_follow`], from the engine's
//! announcement of it); a changed `panels.git_*` setting asks for both panels'. Each panel waits
//! [`DEBOUNCE`] first: a quick walk through folders runs git once, at its end. A newer request of
//! the same panel stops the older one, its git process killed. Both panels showing the same
//! folder share one run: the newer one stops the other's and its answer reaches both. Until the
//! answer the list stands without marks; nothing waits for git but the run's own task.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use rc_vfs::{VfsPath, parse_wsl_unc};
use serde_json::Value;
use tokio::task::AbortHandle;

use super::{
    GitScope, GitView,
    discover::{self, Place},
    marks,
    run::{self, GitError, Host, MIN_VERSION, RunError, Version},
};
use crate::{
    DriveKind, Engine, EngineEvent, Outcome, PanelChange, PanelId,
    drives::drive_key,
    recover::{LockOrRecover as _, LogPanic as _},
    settings::{
        GIT_BRANCH_KEY, GIT_IGNORED_KEY, GIT_PROGRAM_KEY, GIT_STATUS_KEY, stored_bool,
        stored_setting,
    },
};

/// The pause after a panel showed a folder before git is asked.
const DEBOUNCE: Duration = Duration::from_millis(300);
/// How long the look for `.git` may take on a slow drive.
const DISCOVER_LIMIT: Duration = Duration::from_secs(2);

/// The panels' runs, the program found and the versions it answered.
#[derive(Default)]
pub(crate) struct GitDesk {
    /// Each panel's run, by [`PanelId::index`].
    slots: Mutex<[Slot; 2]>,
    /// The `git` of the `PATH`, looked up once per run of the app.
    program: OnceLock<Option<PathBuf>>,
    /// The version each host's git answered, by [`Host::key`].
    versions: Mutex<HashMap<String, Option<Version>>>,
}

/// One panel's run.
#[derive(Default)]
struct Slot {
    /// Counts the requests; a run whose number is no longer this one has been replaced.
    generation: u64,
    task: Option<AbortHandle>,
    /// The folder git runs for, from the run's start to its end.
    folder: Option<PathBuf>,
}

/// The `panels.git_*` settings as one run reads them.
#[derive(Debug, Clone, Default)]
struct GitSettings {
    scope: GitScope,
    ignored: bool,
    branch: bool,
    program: Option<PathBuf>,
}

/// What a run comes to.
enum Found {
    /// Another request replaced it: it shows nothing.
    Replaced,
    /// The folder's marks, or none.
    Marks(Option<GitView>),
}

impl Engine {
    /// Panel `id` shows a folder anew: its git marks are asked for after [`DEBOUNCE`], and a
    /// request of the panel still waiting or running stops. Outside a Tokio runtime nothing runs.
    pub(crate) fn git_follow(&self, id: PanelId) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let mut slots = self.inner.git.slots.lock_or_recover("engine.git");
        let slot = &mut slots[id.index()];
        slot.generation += 1;
        slot.folder = None;
        if let Some(task) = slot.task.take() {
            task.abort();
        }
        let (engine, generation) = (self.clone(), slot.generation);
        let task = runtime.spawn(async move {
            tokio::time::sleep(DEBOUNCE).await;
            engine.git_run(id, generation).await;
        });
        slot.task = Some(task.abort_handle());
    }

    /// Both panels ask for their marks again: a `panels.git_*` setting changed.
    pub(crate) fn git_follow_both(&self) {
        for id in PanelId::BOTH {
            self.git_follow(id);
        }
    }

    /// `panel.git_status`: the marks and the branch the panel knows; starts no process.
    pub(crate) fn git_status(&self, panel: Option<PanelId>) -> Outcome {
        let id = self.resolve(panel);
        let git = self.state().panel(id).git.as_deref().cloned().map(Box::new);
        Outcome::GitStatus { panel: id, git }
    }

    async fn git_run(&self, id: PanelId, generation: u64) {
        let (path, flat) = {
            let state = self.state();
            let panel = state.panel(id);
            (panel.path.clone(), panel.flat.is_some())
        };
        let found = if flat {
            Found::Marks(None)
        } else {
            self.git_view(id, generation, &path).await
        };
        let Found::Marks(view) = found else {
            return;
        };
        if self.git_release(id, generation) {
            self.git_apply(&path, view);
        }
    }

    /// The marks of `path` for panel `id`'s run `generation`.
    async fn git_view(&self, id: PanelId, generation: u64, path: &VfsPath) -> Found {
        let settings = self.git_settings().await;
        let Some(place) = discover::place(path, settings.scope, self.git_on_network(path)) else {
            return Found::Marks(None);
        };
        // A stopped distribution stays stopped: nothing below reads its share.
        let host = match &place {
            Place::Local => self.git_program(&settings).map(Host::Native),
            Place::Wsl(wsl) => wsl_host(&wsl.distro).await,
        };
        let Some(host) = host else {
            return Found::Marks(None);
        };
        let folder = path.path.clone();
        let looked = tokio::time::timeout(
            DISCOVER_LIMIT,
            tokio::task::spawn_blocking(move || discover::repository_root(&folder)),
        )
        .await;
        let Ok(Ok(Some(root))) = looked.map(|joined| joined.log_panic("engine.git_discover"))
        else {
            return Found::Marks(None);
        };
        let (Some(pathspec), Some(dir)) = (
            discover::relative(&root, &path.path),
            host_dir(&place, &root),
        ) else {
            return Found::Marks(None);
        };
        if !self.git_claim(id, generation, &path.path) {
            return Found::Replaced;
        }
        if !self.git_checked(&host).await {
            return Found::Marks(None);
        }
        let fold_case = place == Place::Local && cfg!(any(windows, target_os = "macos"));
        match run::status(&host, &dir, &pathspec, settings.ignored, run::TIME_LIMIT).await {
            Ok(status) => {
                let summary =
                    marks::summarize(&status.items, &pathspec, settings.ignored, fold_case);
                Found::Marks(Some(GitView {
                    folder: path.clone(),
                    branch: settings.branch.then_some(status.branch),
                    marks: summary.marks,
                    whole: summary.whole,
                    deleted: summary.deleted,
                    fold_case,
                }))
            }
            Err(error) => {
                log_failure(&host, &error);
                Found::Marks(None)
            }
        }
    }

    async fn git_settings(&self) -> GitSettings {
        let Ok(db) = self.db() else {
            return GitSettings::default();
        };
        let scope = stored_setting(db, GIT_STATUS_KEY).await;
        let program = stored_setting(db, GIT_PROGRAM_KEY).await;
        GitSettings {
            scope: scope
                .as_ref()
                .and_then(Value::as_str)
                .map_or(GitScope::Off, GitScope::from_setting),
            ignored: stored_bool(db, GIT_IGNORED_KEY).await,
            branch: stored_bool(db, GIT_BRANCH_KEY).await,
            program: program
                .as_ref()
                .and_then(Value::as_str)
                .filter(|program| !program.is_empty())
                .map(PathBuf::from),
        }
    }

    /// Whether `path` lies on another machine: a share by its form, or a network drive.
    fn git_on_network(&self, path: &VfsPath) -> bool {
        let drive = drive_key(path, &self.inner.drives.roots())
            .and_then(|key| self.inner.drives.kind_of(&key));
        rc_vfs::network_path(&path.path.to_string_lossy()).is_some()
            || drive == Some(DriveKind::Network)
    }

    /// The program `panels.git_program` names, else the `PATH`'s `git`.
    fn git_program(&self, settings: &GitSettings) -> Option<PathBuf> {
        if let Some(program) = &settings.program {
            return Some(program.clone());
        }
        let found = self
            .inner
            .git
            .program
            .get_or_init(run::search_system)
            .clone();
        if found.is_none() {
            tracing::info!("no git program on the PATH; the file lists show no git marks");
        }
        found
    }

    /// Whether `host`'s git is at least [`MIN_VERSION`]; asked once per host.
    async fn git_checked(&self, host: &Host) -> bool {
        let key = host.key();
        let known = self
            .inner
            .git
            .versions
            .lock_or_recover("engine.git_versions")
            .get(&key)
            .copied();
        let version = match known {
            Some(version) => version,
            None => match run::version(host).await {
                Ok(version) => {
                    self.inner
                        .git
                        .versions
                        .lock_or_recover("engine.git_versions")
                        .insert(key.clone(), version);
                    version
                }
                Err(error) => {
                    log_failure(host, &error);
                    return false;
                }
            },
        };
        if version.is_some_and(|version| version >= MIN_VERSION) {
            return true;
        }
        let found = version.map_or_else(|| "unknown".to_owned(), |version| version.to_string());
        tracing::info!(
            program = %key,
            %found,
            minimum = %MIN_VERSION,
            "git is older than the version its hardening is checked for; no git marks"
        );
        false
    }

    /// Panel `id`'s run `generation` takes `folder`; a run of the other panel for the same
    /// folder stops, as this one's answer reaches both. `false` when this run was replaced.
    fn git_claim(&self, id: PanelId, generation: u64, folder: &Path) -> bool {
        let mut slots = self.inner.git.slots.lock_or_recover("engine.git");
        if slots[id.index()].generation != generation {
            return false;
        }
        let other = &mut slots[id.other().index()];
        if other.folder.as_deref() == Some(folder) {
            if let Some(task) = other.task.take() {
                task.abort();
            }
            other.generation += 1;
            other.folder = None;
        }
        slots[id.index()].folder = Some(folder.to_path_buf());
        true
    }

    /// Panel `id`'s run `generation` ends; `false` when it was replaced meanwhile.
    fn git_release(&self, id: PanelId, generation: u64) -> bool {
        let mut slots = self.inner.git.slots.lock_or_recover("engine.git");
        let slot = &mut slots[id.index()];
        if slot.generation != generation {
            return false;
        }
        slot.folder = None;
        slot.task = None;
        true
    }

    /// Every panel showing `folder` takes `view`; a flat view takes none. Each changed panel is
    /// announced.
    fn git_apply(&self, folder: &VfsPath, view: Option<GitView>) {
        let view = view.map(Arc::new);
        let mut changed = Vec::new();
        {
            let mut state = self.state();
            for id in PanelId::BOTH {
                let panel = state.panel_mut(id);
                let takes = panel.path == *folder && (view.is_none() || panel.flat.is_none());
                if takes && panel.git != view {
                    panel.git.clone_from(&view);
                    changed.push(id);
                }
            }
        }
        for id in changed {
            self.publish(EngineEvent::Panel {
                panel: id,
                change: PanelChange::Git,
            });
        }
    }
}

/// The git of the WSL distribution `distro`, only while it runs; never on another system.
async fn wsl_host(distro: &str) -> Option<Host> {
    if !cfg!(windows) {
        return None;
    }
    let running = rc_vfs::wsl::running_distros().await;
    if !running.iter().any(|name| name.eq_ignore_ascii_case(distro)) {
        tracing::debug!(%distro, "the distribution does not run; no git marks");
        return None;
    }
    Some(Host::Wsl {
        wsl: rc_vfs::system32_from_env().join("wsl.exe"),
        distro: distro.to_owned(),
    })
}

/// The repository's root as `place`'s git sees it.
fn host_dir(place: &Place, root: &Path) -> Option<String> {
    match place {
        Place::Local => root.to_str().map(str::to_owned),
        Place::Wsl(_) => parse_wsl_unc(root).map(|wsl| wsl.linux_path),
    }
}

/// One log line for a run that gave no marks: a hung or overlong git as a warning, the rest as
/// information. No folder in it: only the program.
fn log_failure(host: &Host, error: &GitError) {
    let program = host.key();
    match error {
        GitError::Run(RunError::TimedOut(_) | RunError::TooLong(_)) | GitError::Porcelain(_) => {
            tracing::warn!(%program, %error, "git status gave no marks");
        }
        _ => tracing::info!(%program, %error, "git status gave no marks"),
    }
}

#[cfg(test)]
#[path = "follow_tests.rs"]
mod tests;
