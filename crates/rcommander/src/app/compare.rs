//! Compare by content and Synchronize dirs (RC-020-04), after Total Commander.
//!
//! `compare.open_diff` (Ctrl+Shift+F3) picks two files as `compare.files` does and the engine
//! announces them; the diff window opens over the window and compares them off the window's
//! thread (`Engine::diff_paths`, for binary and large files `Engine::compare_paths`): two text
//! files side by side, one list so both sides scroll together, changed, deleted and inserted lines
//! tinted, Alt+Down and Alt+Up to the next and previous difference (`diff.rs`, `diff_view.rs`).
//!
//! `sync.open_dialog` (Alt+Shift+F2) opens the synchronize-dirs dialog on the two panels'
//! folders. Compare runs `Engine::plan_sync` in the background, Stop or Escape cancel it; the
//! rows show with their proposed actions, a click or Space changes one, a double-click or F3
//! compares a pair by content. Synchronize asks first - what is copied each way, what is deleted
//! (the deletes can be unticked), whether a replaced file asks - and hands the plan to
//! `Engine::sync_jobs`; the jobs are the file operations' from then on, with their progress
//! window and their conflict and error questions (`sync.rs`, `sync_view.rs`).
//!
//! While either is open it has the keyboard and the mouse; the diff window goes over the dialog.

mod diff;
mod diff_view;
mod sync;
mod sync_input;
mod sync_view;

use std::sync::Arc;

use iced::{Element, Size, Task, widget::stack};
use rc_engine::{
    CancellationToken, Chord, Engine, EngineEvent, JobId, JobSnapshot, Question, VfsPath,
    compare::TextOptions,
    sync::{ItemKind, RowFilter, SyncMode, SyncPlan, proposed_action},
};

pub use self::{
    diff::{DiffOption, DiffWindow, Loaded, Shown},
    sync::{Confirm, Flag, Phase, SyncDialog},
};
use super::{
    App, Effect, Message,
    dialog_windows::DialogWindow,
    file_ops::{FileOpsMsg, FollowUp, Submitted},
};
use crate::{
    i18n::t,
    theme::{Tokens, layout},
};

/// The diff window and the synchronize-dirs dialog, while open.
#[derive(Debug, Default)]
pub struct CompareUi {
    pub diff: Option<DiffWindow>,
    pub sync: Option<SyncDialog>,
    /// Numbers the comparisons of both.
    generation: u64,
}

impl CompareUi {
    fn next_generation(&mut self) -> u64 {
        self.generation += 1;
        self.generation
    }
}

#[derive(Debug, Clone)]
pub enum CompareMsg {
    /// A comparison of the diff window, numbered so.
    DiffLoaded(u64, Result<Loaded, &'static str>),
    DiffOption(DiffOption, bool),
    DiffNext,
    DiffPrevious,
    DiffScroll(isize),
    DiffViewport(Size),
    DiffClose,
    SyncMode(SyncMode),
    SyncFlag(Flag, bool),
    SyncTolerance(String),
    SyncMasks(String),
    SyncCompare,
    SyncStop,
    /// A comparison of the dialog, numbered so.
    SyncPlanned(u64, Result<Arc<SyncPlan>, &'static str>),
    SyncFilter(RowFilter),
    /// A row clicked, by its place in the list.
    SyncClick(usize),
    /// A row's action clicked: the next one it allows.
    SyncCycle(usize),
    /// A row double-clicked: its two files compared by content.
    SyncOpen(usize),
    SyncScroll(isize),
    SyncViewport(Size),
    /// Synchronize: the confirmation opens.
    SyncAsk,
    /// The confirmation's OK.
    SyncRun,
    SyncCancelConfirm,
    /// The queue took the jobs, or the start failed.
    SyncStarted(Result<Vec<JobSnapshot>, &'static str>),
    SyncClose,
}

fn wrap(message: CompareMsg) -> Message {
    Message::Compare(message)
}

/// The diff window's comparison: by line, else by byte. Runs off the window's thread.
pub(super) async fn load(
    engine: Engine,
    left: VfsPath,
    right: VfsPath,
    options: TextOptions,
    cancel: CancellationToken,
) -> Result<Loaded, &'static str> {
    match engine.diff_paths(&left, &right, &options, &cancel).await {
        Ok(Some(diff)) => Ok(Loaded::Text(Arc::new(diff))),
        Ok(None) => engine
            .compare_paths(&left, &right, &cancel)
            .await
            .map(|comparison| Loaded::Bytes(Box::new(comparison)))
            .map_err(|error| error.code()),
        Err(error) => Err(error.code()),
    }
}

impl App {
    /// The engine's announcements open the window and the dialog; while one is open it has the
    /// keys and the clicks. `None` for a message neither takes.
    pub(super) fn compare_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        match message {
            // In a window of its own, it comes to the front with what was asked for (RC-080-15).
            Message::Engine(EngineEvent::CompareFiles { left, right }) => {
                let mut effects = self.open_diff(left.clone(), right.clone());
                effects.extend(self.dialog_window_front(DialogWindow::Compare));
                return Some(effects);
            }
            Message::Engine(EngineEvent::SyncDirs { left, right }) => {
                // The dialog it replaces stops its comparison first (RC-080-44).
                self.close_sync();
                self.compare.sync = Some(SyncDialog::new(left.clone(), right.clone()));
                return Some(self.dialog_window_front(DialogWindow::Compare));
            }
            _ => {}
        }
        let open = self.compare.diff.is_some() || self.compare.sync.is_some();
        let (chord, captured) = match message {
            Message::Key(chord) if open => (chord, false),
            Message::KeyCaptured(chord) if open => (chord, true),
            Message::Press if open => return Some(Vec::new()),
            _ => return None,
        };
        Some(if self.compare.diff.is_some() {
            // Tab walks the focus ring; a key a stop of it took is not the window's again.
            match super::field_walk::walk_key(chord) {
                Some(walk) if !captured => walk,
                _ if captured && super::field_walk::taken(chord) => Vec::new(),
                _ => self.diff_key(chord),
            }
        } else {
            self.sync_key(chord, captured)
        })
    }

    /// The window and the dialog over `base`; in a window of their own they draw there.
    pub(super) fn compare_layer<'a>(
        &'a self,
        base: Element<'a, Message>,
        tokens: &'static Tokens,
    ) -> Element<'a, Message> {
        if self.windowed(DialogWindow::Compare) {
            return base;
        }
        let mut layered = base;
        if let Some(dialog) = &self.compare.sync {
            layered = stack![layered, sync_view::overlay(dialog, tokens)].into();
        }
        if let Some(window) = &self.compare.diff {
            layered = stack![layered, diff_view::overlay(window, tokens)].into();
        }
        layered
    }

    /// The compare window of its own (RC-080-15): the synchronize dialog, the diff window over
    /// it, as over the main window. The question before synchronizing has a window of its own
    /// (RC-080-23, `dialog_windows/questions.rs`).
    pub(super) fn compare_window(&self, tokens: &'static Tokens) -> Vec<Element<'_, Message>> {
        let mut layers = Vec::new();
        if let Some(dialog) = &self.compare.sync {
            layers.push(layout::window_body(sync_view::body(dialog, tokens), tokens));
        }
        if let Some(window) = &self.compare.diff {
            let diff = layout::window_body(diff_view::body(window, tokens), tokens);
            // Over the synchronize dialog it hides it, as the large card does.
            layers.push(iced::widget::opaque(diff));
        }
        layers
    }

    /// The question before synchronizing while it is asked, filling its window of its own.
    pub(super) fn sync_confirm_body(&self) -> Option<Element<'_, Message>> {
        self.compare.sync.as_ref().and_then(sync_view::confirm_body)
    }

    /// The compare window's title: the diff window's while it is open.
    pub(super) fn compare_title(&self) -> &'static str {
        if self.compare.diff.is_some() {
            t!("compare.title")
        } else {
            t!("sync.title")
        }
    }

    /// Opens the diff window on two files; one already open makes way.
    fn open_diff(&mut self, left: VfsPath, right: VfsPath) -> Vec<Effect> {
        let generation = self.compare.next_generation();
        if let Some(old) = self.compare.diff.take() {
            old.cancel.cancel();
        }
        let window = DiffWindow::new(left, right, generation);
        let task = self.diff_task(&window);
        self.compare.diff = Some(window);
        vec![task]
    }

    fn diff_task(&self, window: &DiffWindow) -> Effect {
        let generation = window.generation;
        let comparison = load(
            self.engine.clone(),
            window.left.clone(),
            window.right.clone(),
            window.options,
            window.cancel.clone(),
        );
        Effect::Task(Task::perform(comparison, move |result| {
            wrap(CompareMsg::DiffLoaded(generation, result))
        }))
    }

    pub(super) fn on_compare(&mut self, message: CompareMsg) -> Vec<Effect> {
        match message {
            CompareMsg::DiffLoaded(generation, result) => {
                if let Some(window) = self
                    .compare
                    .diff
                    .as_mut()
                    .filter(|window| window.generation == generation)
                {
                    window.loaded(result);
                }
            }
            CompareMsg::DiffOption(option, on) => {
                let generation = self.compare.next_generation();
                if let Some(window) = self.compare.diff.as_mut() {
                    window.set_option(option, on);
                    window.restart(generation);
                }
                if let Some(window) = &self.compare.diff {
                    return vec![self.diff_task(window)];
                }
            }
            CompareMsg::DiffNext => self.with_diff(|window| {
                window.next();
            }),
            CompareMsg::DiffPrevious => self.with_diff(|window| {
                window.previous();
            }),
            CompareMsg::DiffScroll(rows) => self.with_diff(|window| window.scroll(rows)),
            CompareMsg::DiffViewport(size) => {
                self.with_diff(|window| window.set_page(diff_view::rows_for(size.height)));
            }
            CompareMsg::DiffClose => self.close_diff(),
            other => return self.on_sync(other),
        }
        Vec::new()
    }

    fn with_diff(&mut self, change: impl FnOnce(&mut DiffWindow)) {
        if let Some(window) = self.compare.diff.as_mut() {
            change(window);
        }
    }

    fn close_diff(&mut self) {
        if let Some(window) = self.compare.diff.take() {
            window.cancel.cancel();
        }
    }

    /// A key while the diff window is open: Escape closes it, Alt+Down and Alt+Up jump between
    /// the differences, the cursor keys scroll; every other key stays with the window.
    fn diff_key(&mut self, chord: &Chord) -> Vec<Effect> {
        if chord.ctrl || chord.shift {
            return Vec::new();
        }
        if !chord.alt && chord.key == "Escape" {
            self.close_diff();
            return Vec::new();
        }
        let Some(window) = self.compare.diff.as_mut() else {
            return Vec::new();
        };
        let page = window.rows.page() as isize;
        match (chord.alt, chord.key.as_str()) {
            (true, "ArrowDown") => {
                window.next();
            }
            (true, "ArrowUp") => {
                window.previous();
            }
            (false, "ArrowDown") => window.scroll(1),
            (false, "ArrowUp") => window.scroll(-1),
            (false, "PageDown") => window.scroll(page),
            (false, "PageUp") => window.scroll(-page),
            (false, "Home") => window.home(),
            (false, "End") => window.end(),
            _ => {}
        }
        Vec::new()
    }

    /// "Compare" in the conflict dialog of job `job`: its source and target in the diff window,
    /// over the question, which stays open.
    pub(super) fn compare_question(&mut self, job: JobId) -> Vec<Effect> {
        let pair =
            self.file_ops
                .jobs
                .get(job)
                .and_then(|tracked| match &tracked.snapshot.question {
                    Some(Question::Conflict { source, target, .. })
                        if !source.is_dir && !target.is_dir =>
                    {
                        Some((source.path.clone(), target.path.clone()))
                    }
                    _ => None,
                });
        match pair {
            Some((source, target)) => self.open_diff(source, target),
            None => Vec::new(),
        }
    }

    /// A row's pair of files into the diff window, when both sides are files.
    fn compare_row(&mut self, at: usize) -> Vec<Effect> {
        let Some(dialog) = self.compare.sync.as_ref() else {
            return Vec::new();
        };
        let (Some((_, row)), Some(plan)) = (dialog.row(at), dialog.plan.as_ref()) else {
            return Vec::new();
        };
        if row.left.is_none() || row.right.is_none() || row.kind() != Some(ItemKind::File) {
            return Vec::new();
        }
        let (left, right) = (plan.left.join(&row.path), plan.right.join(&row.path));
        self.open_diff(left, right)
    }

    /// The mode changed: a shown plan takes the new mode's proposals.
    fn set_mode(dialog: &mut SyncDialog, mode: SyncMode) {
        dialog.form.mode = mode;
        if let Some(plan) = dialog.plan.as_mut() {
            let plan = Arc::make_mut(plan);
            plan.options.mode = mode;
            for row in &mut plan.rows {
                row.action = proposed_action(row.state, mode);
            }
        }
    }

    /// The queue took the jobs: they are the file operations' now - the first in the progress
    /// window, the others in the background bar - and the dialog closes.
    fn sync_started(&mut self, result: Result<Vec<JobSnapshot>, &'static str>) -> Vec<Effect> {
        let jobs = match result {
            Ok(jobs) => jobs,
            Err(code) => {
                if let Some(dialog) = self.compare.sync.as_mut() {
                    dialog.phase = Phase::Failed(code);
                }
                return Vec::new();
            }
        };
        if jobs.is_empty() {
            if let Some(dialog) = self.compare.sync.as_mut() {
                dialog.phase = Phase::Nothing;
            }
            return Vec::new();
        }
        self.compare.sync = None;
        let panel = self.active;
        let mut effects = Vec::new();
        for (place, job) in jobs.into_iter().enumerate() {
            let submitted = Submitted {
                panel,
                follow: FollowUp::Refresh,
                background: place > 0,
                result: Ok(vec![job]),
                refused: None,
            };
            effects.extend(self.on_file_ops(FileOpsMsg::Submitted(Box::new(submitted))));
        }
        effects
    }
}

#[cfg(test)]
#[path = "compare_tests.rs"]
mod tests;
