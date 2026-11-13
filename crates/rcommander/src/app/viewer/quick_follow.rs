//! Quick View following its panel's cursor (RC-110-28). On a local folder a step of the cursor
//! shows its entry at once. Anywhere else - an SFTP or FTP server, a device, an archive - it
//! asks for nothing: it waits for the cursor to rest for `viewer.quick_view_pause_ms`
//! ([`QuickPause`], 300 ms as the panel beside a tree; 0 for no wait), so an arrow key held
//! through 200 entries of an SFTP folder lists or opens the one it stops at and none on the
//! way. Each move has a turn; a pause that ends for an older turn counts for nothing.
//!
//! The request behind what is shown - a directory's listing, a file's open - is a task that
//! ends when its [`Running`] is dropped: when the cursor leaves the entry before the answer
//! came, when the next request takes its place, and when Quick View is switched off. The
//! engine has no handle of its own for a listing or an open; the task's future is dropped at
//! its next await, and a viewer plugin's call ends with the lister's flag as before.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use iced::{Task, task};
use rc_db::Db;
use rc_engine::{
    Command, Outcome, PanelId, VfsPath,
    settings::{QUICK_VIEW_PAUSE_DEFAULT_MS, QUICK_VIEW_PAUSE_KEY, stored_setting},
};
use rc_vfs::BackendId;

use super::{
    ViewerMessage,
    lister::Lister,
    quick::{self, DirSummary, Quick, QuickShown},
    wrap,
};
use crate::app::{App, Effect, Message};

/// `viewer.quick_view_pause_ms`: how long the cursor rests on an entry that is not on a local
/// disk before Quick View reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuickPause(pub Duration);

impl Default for QuickPause {
    /// The schema's default, until the setting is read.
    fn default() -> Self {
        Self(Duration::from_millis(
            QUICK_VIEW_PAUSE_DEFAULT_MS.unsigned_abs(),
        ))
    }
}

/// Whether a changed setting is the pause.
pub fn concerns(key: &str) -> bool {
    key == QUICK_VIEW_PAUSE_KEY
}

/// The stored pause; the schema holds it to its bounds.
pub async fn load(db: Db) -> QuickPause {
    let stored = stored_setting(&db, QUICK_VIEW_PAUSE_KEY).await;
    stored
        .as_ref()
        .and_then(serde_json::Value::as_u64)
        .map_or_else(QuickPause::default, |ms| {
            QuickPause(Duration::from_millis(ms))
        })
}

/// The request behind what Quick View shows.
#[derive(Debug)]
pub struct Running {
    task: task::Handle,
    /// The answer came.
    done: Arc<AtomicBool>,
}

impl Drop for Running {
    /// Nobody waits for the answer any more: the task ends.
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Running {
    /// `task` as one that ends when its `Running` is dropped.
    fn of(task: Task<Message>) -> (Task<Message>, Self) {
        let done = Arc::new(AtomicBool::new(false));
        let answered = Arc::clone(&done);
        let (task, handle) = task.abortable();
        // Marked past the point where the task can end: an answer that counts as come is on
        // its way to the window.
        let task = task.map(move |message| {
            answered.store(true, Ordering::Relaxed);
            message
        });
        (task, Self { task: handle, done })
    }

    /// Whether the answer came: what is shown is the entry, not the wait for it.
    pub fn finished(&self) -> bool {
        self.done.load(Ordering::Relaxed)
    }
}

impl App {
    /// Reads `viewer.quick_view_pause_ms`.
    pub(in crate::app) fn quick_pause_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(load(db), |pause| wrap(ViewerMessage::QuickPause(pause)))
    }

    /// Ctrl+Q from `panel`: Quick View of its cursor in the other panel, or off again.
    pub(super) fn toggle_quick(&mut self, panel: PanelId) -> Vec<Effect> {
        if self.viewers.quick.take().is_some() {
            return Vec::new();
        }
        self.viewers.quick = Some(Quick::new(panel));
        self.quick_show()
    }

    /// What Quick View is to show for its panel's cursor; the outer `None` without a Quick
    /// View, the inner one on `..` or in an empty directory.
    fn quick_wanted(&self) -> Option<Option<(VfsPath, bool)>> {
        let source = self.viewers.quick.as_ref()?.source;
        let pane = self.pane(source);
        let entry = if pane.on_parent() {
            None
        } else {
            pane.entries().get(pane.cursor())
        };
        Some(quick::wanted(&pane.state.path, entry))
    }

    /// The panel's cursor may have moved: Quick View shows the entry at once on a local
    /// folder and once the cursor rested there anywhere else, and the request of the entry it
    /// left ends now.
    pub(super) fn quick_follow(&mut self) -> Vec<Effect> {
        let Some(wanted) = self.quick_wanted() else {
            return Vec::new();
        };
        let pause = self.viewers.quick_pause.0;
        let Some(quick) = self.viewers.quick.as_mut() else {
            return Vec::new();
        };
        let turn = quick.moved();
        let Some((path, _)) = wanted else {
            quick.clear();
            return Vec::new();
        };
        if quick.path() == Some(&path) {
            return Vec::new();
        }
        if path.backend == BackendId::Local || pause.is_zero() {
            // Nothing to wait for; the request before ends with the one that takes its place.
            return self.quick_show();
        }
        quick.leave();
        let rest = Task::perform(tokio::time::sleep(pause), move |()| {
            wrap(ViewerMessage::QuickRested(turn))
        });
        vec![Effect::Task(rest)]
    }

    /// The pause after the cursor's move of `turn` ended: shown, when no later move came.
    pub(super) fn quick_rested(&mut self, turn: u64) -> Vec<Effect> {
        let last = self.viewers.quick.as_ref().map(|quick| quick.turn);
        if last == Some(turn) {
            self.quick_show()
        } else {
            Vec::new()
        }
    }

    /// Quick View catches up with its panel's cursor now: a new file opens, a directory is
    /// counted, and the request before ends.
    fn quick_show(&mut self) -> Vec<Effect> {
        let Some(wanted) = self.quick_wanted() else {
            return Vec::new();
        };
        let id = self.viewers.next_id();
        let Some(quick) = self.viewers.quick.as_mut() else {
            return Vec::new();
        };
        let source = quick.source;
        let Some((path, is_dir)) = wanted else {
            quick.clear();
            return Vec::new();
        };
        if quick.path() == Some(&path) {
            return Vec::new();
        }
        let task = if is_dir {
            quick.shown = QuickShown::Dir {
                path: path.clone(),
                summary: None,
            };
            self.count_task(source, path)
        } else {
            quick.show_file(Lister::new(id, source, path.clone()));
            self.open_task(id, path)
        };
        let (task, running) = Running::of(task);
        if let Some(quick) = self.viewers.quick.as_mut() {
            // The request before ends with its `Running`.
            quick.running = Some(running);
        }
        vec![Effect::Task(task)]
    }

    /// The directory `path` listed and counted for Quick View.
    fn count_task(&self, source: PanelId, path: VfsPath) -> Task<Message> {
        let engine = self.engine.clone();
        let listing = Command::ListDir {
            panel: Some(source),
            path: Some(path.path.to_string_lossy().into_owned()),
        };
        let counting = async move {
            match engine.dispatch(listing).await {
                Ok(Outcome::Listing { entries, .. }) => Ok(DirSummary::of(&entries)),
                Ok(_) => Ok(DirSummary::default()),
                Err(error) => Err(error.code()),
            }
        };
        Task::perform(counting, move |summary| {
            wrap(ViewerMessage::Summary(path, summary))
        })
    }
}

#[cfg(test)]
#[path = "quick_follow_tests.rs"]
mod tests;
