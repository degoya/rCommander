//! Remote entries made local before they leave the window (RC-080-01): a file of an FTP or SFTP
//! panel or of an archive cannot be dragged out or put on the clipboard as it is, so it is
//! copied into the session's temporary folder first (`temp.rs`) - by `ops.copy`, a job of the
//! queue with its progress window, its Cancel and its conflict questions - and the copies go
//! out instead. The fetch itself is data without a window; the app's side of it - the question,
//! the session folder, the job and its end - is the `impl App` below.
//!
//! One session folder is made at a time: fetches asked for while it is being made wait in its
//! place, the last one asked for, and a session that has its folder keeps it (RC-080-38).

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::SystemTime,
};

use iced::Task;
use rc_engine::{
    Command, DirSize, Entry, JobChange, JobId, JobKind, JobOptions, JobSnapshot, PanelId,
    PanelState, TransferRequest, VfsPath,
};
use rc_vfs::BackendId;

use super::{DragMsg, start_drag, temp, wrap};
use crate::{
    app::{
        App, Effect,
        file_ops::{FollowUp, Submit},
    },
    i18n::t,
};

/// Above this many bytes - or with a folder of unknown size - the person is asked first.
pub const ASK_ABOVE: u64 = 256 * 1024 * 1024;

/// What fetched entries are for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// Dragged out of the window.
    Drag,
    /// Put on the clipboard (Ctrl+C).
    Clipboard,
}

/// The entries of a folder as the listing showed them: fetched copies are used again only while
/// the names, sizes and times are the same.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FetchKey {
    folder: VfsPath,
    entries: Vec<(String, u64, Option<SystemTime>)>,
}

/// One fetch: the entries of a panel's folder, what they are for and, once it runs, where they go.
#[derive(Debug, Clone, PartialEq)]
pub struct Fetch {
    pub panel: PanelId,
    pub names: Vec<String>,
    pub purpose: Purpose,
    pub key: FetchKey,
    /// Their size, `None` when a folder among them was not counted.
    pub size: Option<u64>,
    /// The temporary folder the job copies into, once it was started.
    pub folder: Option<PathBuf>,
    /// A job of the fetch - one per entry, RC-010-14 - ended with a failure or a skipped file.
    pub spoiled: bool,
}

impl Fetch {
    /// A fetch of `names` of the panel `panel` with the state `state`.
    pub fn new(panel: PanelId, state: &PanelState, names: Vec<String>, purpose: Purpose) -> Self {
        let by_name: HashMap<&str, &Entry> = state
            .entries
            .iter()
            .map(|entry| (entry.name.as_str(), entry))
            .collect();
        let mut entries = Vec::with_capacity(names.len());
        let mut size = Some(0_u64);
        for name in &names {
            let entry = by_name.get(name.as_str()).copied();
            let bytes = match entry {
                Some(entry) if !entry.is_dir_like() => Some(entry.size),
                Some(_) => match state.dir_sizes.get(name) {
                    Some(DirSize::Bytes(bytes)) => Some(*bytes),
                    _ => None,
                },
                None => None,
            };
            size = size
                .zip(bytes)
                .map(|(sum, bytes)| sum.saturating_add(bytes));
            entries.push((
                name.clone(),
                entry.map_or(0, |entry| entry.size),
                entry.and_then(|entry| entry.modified),
            ));
        }
        Self {
            panel,
            key: FetchKey {
                folder: state.path.clone(),
                entries,
            },
            names,
            purpose,
            size,
            folder: None,
            spoiled: false,
        }
    }

    /// Whether the person is asked before the fetch starts.
    pub fn asks(&self) -> bool {
        self.size.is_none_or(|size| size > ASK_ABOVE)
    }
}

/// Whether entries of `folder` must be fetched before they can leave the window.
pub fn remote(folder: &VfsPath) -> bool {
    folder.backend != BackendId::Local
}

/// Whether a fetch from a panel whose other panel shows `other` can be named: `ops.copy` takes
/// its target on the other panel's backend, so that one must be local.
pub fn possible(other: &VfsPath) -> bool {
    other.backend == BackendId::Local
}

/// The `ops.copy` of `names` of panel `panel` into the local `folder`; see [`possible`].
pub fn command(panel: PanelId, names: &[String], folder: &Path) -> Command {
    Command::OpsCopy(TransferRequest {
        panel: Some(panel),
        sources: names.to_vec(),
        target: Some(folder.display().to_string()),
        options: JobOptions::default(),
    })
}

/// Whether `snapshot`, a finished job of a fetch, went through: completed, nothing failed or
/// skipped.
pub fn clean(snapshot: &JobSnapshot) -> bool {
    snapshot.state == rc_engine::JobState::Completed
        && snapshot.failures.is_empty()
        && snapshot.skipped == 0
}

/// The local folder a copy job wrote into: how a finished job is known as a fetch.
pub fn target_of(snapshot: &JobSnapshot) -> Option<&Path> {
    match &snapshot.kind {
        JobKind::Copy { target, .. } if target.backend == BackendId::Local => {
            Some(target.path.as_path())
        }
        _ => None,
    }
}

/// The fetched copies of `names` in `folder`: each entry under its own name (a name of the flat
/// view, `sub/file`, by its last part), each once.
pub fn fetched_paths(folder: &Path, names: &[String]) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::with_capacity(names.len());
    let mut seen: HashSet<PathBuf> = HashSet::with_capacity(names.len());
    for name in names {
        if let Some(last) = Path::new(name).file_name() {
            let path = folder.join(last);
            if seen.insert(path.clone()) {
                paths.push(path);
            }
        }
    }
    paths
}

impl App {
    /// Remote entries of `panel` for `purpose`: copies fetched before for the same listing go
    /// out again; otherwise they are fetched, after a question above the size limit.
    pub(super) fn fetch(
        &mut self,
        panel: PanelId,
        names: Vec<String>,
        purpose: Purpose,
    ) -> Vec<Effect> {
        if !possible(&self.pane(panel.other()).state.path) {
            self.pane_mut(panel).error = Some(t!("drag.needs_local").to_owned());
            return Vec::new();
        }
        let fetch = Fetch::new(panel, &self.pane(panel).state, names, purpose);
        if let Some(folder) = self.drag.fetched.get(&fetch.key).cloned() {
            return self.use_fetched(&fetch, folder);
        }
        if fetch.asks() {
            self.drag.question = Some(fetch);
            return Vec::new();
        }
        self.begin_fetch(fetch)
    }

    /// The size question for `names` of `panel`, as a drag of remote entries asks it - for the
    /// focus ring's tests (`field_walk`), which have no server to drag from.
    #[cfg(test)]
    pub(in crate::app) fn ask_fetch(&mut self, panel: PanelId, names: Vec<String>) {
        let fetch = Fetch::new(panel, &self.pane(panel).state, names, Purpose::Drag);
        self.drag.question = Some(fetch);
    }

    /// Starts `fetch`, once the session has its temporary folder. While that folder is being
    /// made the fetch waits in place of the one waiting before; no second folder is asked for,
    /// whose answer would hand the fetch in once more.
    pub(super) fn begin_fetch(&mut self, fetch: Fetch) -> Vec<Effect> {
        if self.drag.temp.session().is_some() {
            return self.submit_fetch(fetch);
        }
        let making = self
            .drag
            .running
            .as_ref()
            .is_some_and(|waiting| waiting.folder.is_none());
        self.drag.running = Some(fetch);
        if making {
            return Vec::new();
        }
        let make = Task::perform(temp::make_session_off_thread(), |result| {
            wrap(DragMsg::Session(result))
        });
        vec![Effect::Task(make)]
    }

    /// The session folder was made, or could not be: the waiting fetch starts. A session that
    /// has its folder keeps it; one made besides is removed again.
    pub(super) fn session_made(&mut self, result: Result<PathBuf, String>) -> Vec<Effect> {
        let mut effects = Vec::new();
        match result {
            Ok(folder) if self.drag.temp.session().is_some() => {
                let removed = Task::future(temp::remove_off_thread(folder)).discard();
                effects.push(Effect::Task(removed));
            }
            Ok(folder) => self.drag.temp.set_session(folder),
            // The panel says so below; the reason goes into the log.
            Err(error) => tracing::warn!(%error, "the drag session folder was not made"),
        }
        let Some(fetch) = self.drag.running.take_if(|fetch| fetch.folder.is_none()) else {
            return effects;
        };
        if self.drag.temp.session().is_none() {
            self.pane_mut(fetch.panel).error = Some(t!("drag.no_temp").to_owned());
            return effects;
        }
        effects.extend(self.submit_fetch(fetch));
        effects
    }

    /// The fetch's job: `ops.copy` into a new folder of the session, in the progress window.
    pub(super) fn submit_fetch(&mut self, mut fetch: Fetch) -> Vec<Effect> {
        let Some(folder) = self.drag.temp.next_folder() else {
            return Vec::new();
        };
        let command = command(fetch.panel, &fetch.names, &folder);
        let panel = fetch.panel;
        fetch.folder = Some(folder);
        self.drag.running = Some(fetch);
        vec![Effect::Ops(Submit {
            panel,
            command,
            follow: FollowUp::Refresh,
            background: false,
        })]
    }

    /// A job's change: when the running fetch's jobs - one per entry (RC-010-14) - are over and
    /// completed without a failure or a skipped file, its copies go where they were wanted. What
    /// went wrong the file operations report.
    pub(in crate::app) fn drag_job(&mut self, job: JobId, change: &JobChange) -> Vec<Effect> {
        let JobChange::Finished { .. } = change else {
            return Vec::new();
        };
        let Some(folder) = self
            .drag
            .running
            .as_ref()
            .and_then(|fetch| fetch.folder.clone())
        else {
            return Vec::new();
        };
        let Ok(snapshot) = self.engine.ops().job(job) else {
            return Vec::new();
        };
        let ours = |job: &JobSnapshot| target_of(job) == Some(folder.as_path());
        if !ours(&snapshot) {
            return Vec::new();
        }
        let others = self.engine.ops().jobs();
        let Some(fetch) = self.drag.running.as_mut() else {
            return Vec::new();
        };
        fetch.spoiled |= !clean(&snapshot);
        if others
            .iter()
            .any(|job| ours(job) && !job.state.is_finished())
        {
            return Vec::new();
        }
        let Some(fetch) = self.drag.running.take().filter(|fetch| !fetch.spoiled) else {
            return Vec::new();
        };
        self.drag.fetched.insert(fetch.key.clone(), folder.clone());
        self.use_fetched(&fetch, folder)
    }

    /// The copies of `fetch` in `folder` go out: dragged, or onto the clipboard.
    fn use_fetched(&mut self, fetch: &Fetch, folder: PathBuf) -> Vec<Effect> {
        let paths = fetched_paths(&folder, &fetch.names);
        match fetch.purpose {
            Purpose::Drag => start_drag(fetch.panel, paths, Some(folder)),
            Purpose::Clipboard => self.put_on_clipboard(paths, false),
        }
    }
}

#[cfg(test)]
#[path = "fetch_tests.rs"]
mod tests;
