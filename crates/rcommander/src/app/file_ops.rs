//! The file operations of the function keys (RC-010-07), after Total Commander: F5 copies and F6
//! moves from the active panel into the other one, Shift+F5 copies beside the file, Shift+F6
//! renames in the list, F7 makes a folder, F8 and Delete delete (into the recycle bin; with
//! Shift for good), Shift+F4 makes a text file; F3 and F4 open the external viewer and editor.
//!
//! Every key runs a registry command. The dialog keys run `ops.dialog`, which the engine
//! announces ([`rc_engine::EngineEvent::FileDialog`]); the dialog opens here (`dialog.rs`), and
//! OK runs the operation's own command - `ops.copy_to`, `ops.move_to`, `ops.mkdir`,
//! `ops.delete`, `file.open_external`. A job the window started is followed (`jobs.rs`): its
//! progress in a compact window or the background bar, its questions - a conflict, a failing
//! file - in a dialog that answers through `ops.answer`, and when it is over both panels are read
//! again (`run.rs`). Drawing is `view.rs` - in windows of their own `window.rs` (RC-080-23) -,
//! the function-key bar `fkey_bar.rs`, the background bar `strip.rs`.

mod dialog;
mod fkey_bar;
mod jobs;
mod pace;
mod question;
mod run;
mod strip;
mod totals;
mod view;
mod window;

use iced::keyboard::Modifiers;
use rc_engine::{
    Answer, Chord, Command, ConflictChoice, Engine, ErrorChoice, FileDialog, JobId, JobSnapshot,
    Outcome, PanelId, check_name,
};

pub use self::dialog::{Dialog, FollowUp, InlineRename, Preset};
use self::jobs::Jobs;
pub use self::strip::concerns as settings_concern;
pub(in crate::app) use self::view::duration;
pub(in crate::app) use self::window::dialog_window;
use super::{App, Effect, Message};
use crate::i18n::error_text;

/// The id of the open dialog's text field.
pub(in crate::app) const FIELD_ID: &str = "rcommander.file_ops.field";
/// The id of the rename editor in the list.
const RENAME_ID: &str = "rcommander.file_ops.rename";
/// The id of the "rename to" field of the conflict dialog.
const PROPOSAL_ID: &str = "rcommander.file_ops.proposal";

/// The file operations' part of the window.
#[derive(Debug, Default)]
pub struct FileOps {
    /// The open dialog of F5, Shift+F5, F6, F7, F8 or Shift+F4.
    pub dialog: Option<Dialog>,
    /// Shift+F6's editor in the list.
    pub rename: Option<InlineRename>,
    pub jobs: Jobs,
    /// The modifiers held: the function-key bar shows what they make of each key.
    modifiers: Modifiers,
    /// The function-key bar is switched off in the settings.
    pub hide_bar: bool,
    /// The background bar lists its jobs, not only how many there are.
    pub expanded: bool,
    /// How many jobs the background bar lists (`queue.strip_rows`); `None` before it is read.
    pub strip_rows: Option<usize>,
    /// The background bar's jobs, the queue window's order (RC-010-25, `strip.rs`).
    pub bar: Vec<rc_engine::QueueEntry>,
    /// `queue.show_at_once` is off: the queue window lists the queue's lists alone.
    pub hide_at_once: bool,
    /// `queue.total_speed` off: the queue window shows the selected job's speed alone.
    pub hide_total_speed: bool,
    /// `queue.hide_single_lane`: the queue window leaves out a lane column all its jobs share.
    pub hide_single_lane: bool,
    /// The queue list F2 put a copy or move into last (RC-010-15).
    pub queue_target: rc_engine::QueueListId,
    /// `checksum.enter_verifies` is off: Enter opens a checksum file as any other (RC-020-10).
    pub enter_opens_checksums: bool,
    /// `mkdir.prefill_cursor_name` is off: F7 starts with an empty name.
    pub mkdir_blank: bool,
    /// The jobs whose archive password was answered, until they take the answer: their prompt
    /// does not open again meanwhile (`archive_ops/password.rs`, RC-080-44).
    pub passwords_answered: Vec<JobId>,
}

impl FileOps {
    /// Whether a dialog has the keys and the clicks: an input dialog, a question, the progress
    /// window.
    pub fn modal(&self) -> bool {
        self.dialog.is_some() || self.jobs.asking().is_some() || self.jobs.foreground().is_some()
    }
}

#[derive(Debug, Clone)]
pub enum FileOpsMsg {
    /// The text of the open dialog changed.
    Input(String),
    Preset(Preset),
    Verify(bool),
    KeepAttributes(bool),
    /// The queue list F2 puts the job into.
    QueueList(rc_engine::QueueListId),
    /// OK, or "Queue" (`background`): the dialog's command runs.
    Start {
        background: bool,
    },
    /// Cancel in a dialog.
    Close,
    RenameInput(String),
    /// Enter in the rename editor.
    RenameSubmit,
    /// The engine took a job, or refused it.
    Submitted(Box<Submitted>),
    Pause(JobId),
    Resume(JobId),
    Abort(JobId),
    /// The job's progress window goes into the background bar, or comes back.
    Background(JobId),
    Foreground(JobId),
    /// The background bar lists the jobs, or only counts them.
    Expand(bool),
    /// Pause all, Resume all or Cancel all in the background bar.
    All(strip::All),
    Conflict {
        job: JobId,
        choice: ConflictChoice,
        for_all: bool,
    },
    ProposalInput(JobId, String),
    /// "Rename to" with the edited name.
    RenameTo(JobId),
    /// The answer to a failing file; `all` skips every later one without asking.
    Error {
        job: JobId,
        choice: ErrorChoice,
        all: bool,
    },
    /// The keyboard's modifiers changed.
    Modifiers(Modifiers),
    /// A click on the function-key bar: the key's command.
    Key(Box<Command>),
    /// The function-key bar, F7's name, the background bar's rows and whether the queue window
    /// lists the jobs run at once, as stored.
    Settings(strip::Stored),
    /// "Compare" in the conflict dialog: the source and the target in the diff window, the
    /// question still open (RC-020-04).
    Compare(JobId),
}

/// A job command for the engine, with what the window does with its job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submit {
    pub panel: PanelId,
    pub command: Command,
    pub follow: FollowUp,
    pub background: bool,
}

/// What became of a [`Submit`]: its job, or its jobs in their order - one per source
/// (RC-010-14).
#[derive(Debug, Clone)]
pub struct Submitted {
    pub panel: PanelId,
    pub follow: FollowUp,
    pub background: bool,
    pub result: Result<Vec<JobSnapshot>, &'static str>,
    /// The command, when a locked vault refused it: the unlock prompt submits it again
    /// (RC-050-03).
    pub refused: Option<Command>,
}

impl Submit {
    /// Hands the command to `engine`; the job it accepted.
    pub async fn run(self, engine: Engine) -> Submitted {
        // Kept for a refusal; a command is a few words.
        let again = self.command.clone();
        let mut refused = None;
        let result = match engine.dispatch(self.command).await {
            Ok(Outcome::Job(job)) => Ok(vec![*job]),
            Ok(Outcome::Jobs { jobs }) if !jobs.is_empty() => Ok(jobs),
            // The ops commands answer with their jobs; anything else is no job to follow.
            Ok(_) => Err("io"),
            Err(error) => {
                if error.code() == "vault_locked" {
                    refused = Some(again);
                }
                Err(error.code())
            }
        };
        Submitted {
            panel: self.panel,
            follow: self.follow,
            background: self.background,
            result,
            refused,
        }
    }
}

fn wrap(message: FileOpsMsg) -> Message {
    Message::FileOps(message)
}

/// Gives the field `id` the keys, with its first `select` characters selected (all of them
/// when `None`), so typing replaces them.
fn focus(id: &'static str, select: Option<usize>) -> Effect {
    Effect::Task(window::focus_task(id, select))
}

impl App {
    /// While a dialog of the file operations is open it has the keys - Escape closes it, Enter
    /// (one the field has not used already) confirms it - and clicks reach no panel; the
    /// rename editor has the keys too. `None` for a message the file operations do not take.
    pub(super) fn file_ops_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        if self.archive.is_some() {
            // The password prompt of an unpacking job lies over its progress window.
            return None;
        }
        let ops = &self.file_ops;
        let (chord, captured) = match message {
            Message::Key(chord) => (chord, false),
            Message::KeyCaptured(chord) => (chord, true),
            Message::Press if ops.modal() => return Some(Vec::new()),
            _ => return None,
        };
        let plain = chord.is_plain();
        let key = |name: &str| plain && chord.key == name;
        // Tab and Shift+Tab walk the focus ring of the question, the dialog or the progress
        // window (RC-080-16).
        if !captured
            && ops.modal()
            && let Some(effects) = super::field_walk::walk_key(chord)
        {
            return Some(effects);
        }
        // In its window of its own the question leaves the keys to the dialog's or the progress
        // window while that has the keyboard (RC-080-23).
        if let Some(job) = ops.jobs.asking().filter(|_| self.job_question_has_keys()) {
            // A question waits for one of its buttons; Escape is its Cancel (RC-080-15).
            return Some(match question::cancel(job).filter(|_| key("Escape")) {
                Some(cancel) => self.on_file_ops(cancel),
                None => Vec::new(),
            });
        }
        if let Some(dialog) = &ops.dialog {
            return Some(if key("Escape") {
                self.file_ops.dialog = None;
                Vec::new()
            } else if key("F2") && dialog.queues() {
                // Total Commander's F2 in the copy dialog: into the queue, behind the others.
                self.start(true)
            } else if key("Enter") && !captured {
                self.start(false)
            } else {
                Vec::new()
            });
        }
        if let Some(job) = ops.jobs.foreground().map(jobs::Tracked::id) {
            // F2 as in the copy dialog: the job goes on behind the window (RC-010-26).
            if key("Escape") || key("F2") {
                self.file_ops.jobs.set_foreground(job, false);
            }
            return Some(Vec::new());
        }
        ops.rename.as_ref()?;
        Some(if key("Escape") {
            self.file_ops.rename = None;
            Vec::new()
        } else if key("Enter") && !captured {
            self.rename_submit()
        } else {
            Vec::new()
        })
    }

    /// `ops.dialog` was dispatched: the dialog opens over the window, the rename editor in the
    /// list. Nothing opens over another dialog or where there is nothing to work on.
    pub(super) fn open_file_dialog(&mut self, panel: PanelId, kind: FileDialog) -> Vec<Effect> {
        if self.file_ops.modal() || self.file_ops.rename.is_some() {
            return Vec::new();
        }
        let pane = self.pane(panel);
        let (state, on_parent) = (pane.state.clone(), pane.on_parent());
        if kind == FileDialog::Rename {
            self.file_ops.rename = InlineRename::open(panel, &state, on_parent);
            return match &self.file_ops.rename {
                Some(rename) => vec![focus(RENAME_ID, Some(rename.selected))],
                None => Vec::new(),
            };
        }
        // A server's location as `sftp://<name>/...`, so the target keeps its backend.
        let other = self.location_text(&self.pane(panel.other()).state.path);
        self.file_ops.dialog = Dialog::open(kind, panel, &state, on_parent, &other);
        if let Some(dialog) = &mut self.file_ops.dialog {
            dialog.offer_lists(self.engine.ops().lists(), self.file_ops.queue_target);
            if kind == FileDialog::Mkdir && self.file_ops.mkdir_blank {
                dialog.text.clear();
            }
        }
        match &self.file_ops.dialog {
            Some(dialog) if dialog.has_field() => vec![focus(FIELD_ID, dialog.selection())],
            _ => Vec::new(),
        }
    }

    /// A message of the file operations.
    pub(super) fn on_file_ops(&mut self, message: FileOpsMsg) -> Vec<Effect> {
        match message {
            FileOpsMsg::Input(text) => {
                if let Some(dialog) = &mut self.file_ops.dialog {
                    dialog.text = text;
                    dialog.error = None;
                }
            }
            FileOpsMsg::Preset(preset) => {
                if let Some(dialog) = &mut self.file_ops.dialog {
                    dialog.preset = preset;
                }
            }
            FileOpsMsg::Verify(on) => {
                if let Some(dialog) = &mut self.file_ops.dialog {
                    dialog.verify = on;
                }
            }
            FileOpsMsg::KeepAttributes(on) => {
                if let Some(dialog) = &mut self.file_ops.dialog {
                    dialog.keep_attributes = on;
                }
            }
            FileOpsMsg::QueueList(list) => {
                if let Some(dialog) = &mut self.file_ops.dialog {
                    dialog.queue = list;
                }
            }
            FileOpsMsg::Start { background } => return self.start(background),
            FileOpsMsg::Close => self.file_ops.dialog = None,
            FileOpsMsg::RenameInput(text) => {
                if let Some(rename) = &mut self.file_ops.rename {
                    rename.text = text;
                }
            }
            FileOpsMsg::RenameSubmit => return self.rename_submit(),
            FileOpsMsg::Submitted(submitted) => return self.submitted(*submitted),
            FileOpsMsg::Pause(job) => return self.steer_all(job, |job| Command::OpsPause { job }),
            FileOpsMsg::Resume(job) => {
                return self.steer_all(job, |job| Command::OpsResume { job });
            }
            FileOpsMsg::Abort(job) => return self.steer_all(job, |job| Command::OpsCancel { job }),
            FileOpsMsg::Background(job) => self.file_ops.jobs.set_foreground(job, false),
            FileOpsMsg::Foreground(job) => self.file_ops.jobs.set_foreground(job, true),
            FileOpsMsg::Expand(open) => self.file_ops.expanded = open,
            FileOpsMsg::All(all) => return self.steer_background(all),
            FileOpsMsg::Conflict {
                job,
                choice,
                for_all,
            } => return self.answer(job, Answer::Conflict { choice, for_all }),
            FileOpsMsg::ProposalInput(job, text) => {
                if let Some(tracked) = self.file_ops.jobs.get_mut(job) {
                    tracked.rename_to = text;
                    tracked.note = None;
                }
            }
            FileOpsMsg::RenameTo(job) => return self.rename_to(job),
            FileOpsMsg::Error { job, choice, all } => {
                if all {
                    self.file_ops.jobs.skip_errors(job);
                }
                return self.answer(job, Answer::Error { choice });
            }
            FileOpsMsg::Modifiers(modifiers) => self.file_ops.modifiers = modifiers,
            FileOpsMsg::Key(command) => {
                let active = self.active;
                return self
                    .pane_mut(active)
                    .through_parent_row(*command)
                    .map(|command| Effect::Dispatch(active, vec![command]))
                    .into_iter()
                    .collect();
            }
            FileOpsMsg::Settings(settings) => {
                self.file_ops.hide_bar = !settings.bar;
                self.file_ops.mkdir_blank = !settings.prefill;
                self.file_ops.strip_rows = Some(settings.rows);
                self.file_ops.hide_at_once = !settings.at_once;
                self.file_ops.hide_total_speed = !settings.total_speed;
                self.file_ops.hide_single_lane = settings.hide_single_lane;
                self.refresh_queue_window();
            }
            FileOpsMsg::Compare(job) => return self.compare_question(job),
        }
        Vec::new()
    }

    /// OK in the dialog: its command, or the reason it has none shown in the dialog. "Queue"
    /// (F2) puts a copy or move into the chosen queue list and remembers it (RC-010-15).
    fn start(&mut self, background: bool) -> Vec<Effect> {
        let Some(dialog) = &mut self.file_ops.dialog else {
            return Vec::new();
        };
        let submitted = if background && dialog.queues() {
            self.file_ops.queue_target = dialog.queue;
            dialog.submit_queued()
        } else {
            dialog.submit()
        };
        let (command, follow) = match submitted {
            Ok(submitted) => submitted,
            Err(code) => {
                dialog.error = Some(code);
                return Vec::new();
            }
        };
        let (panel, kind) = (dialog.panel, dialog.kind);
        self.file_ops.dialog = None;
        if kind == FileDialog::NewText {
            // No job: the file is made and handed to the editor at once.
            let mut commands = vec![command];
            commands.extend(run::follow_commands(panel, &follow, true));
            return vec![Effect::Dispatch(panel, commands)];
        }
        vec![Effect::Ops(Submit {
            panel,
            command,
            follow,
            background,
        })]
    }

    /// Enter in the rename editor: the rename as a background job; a name that will not do
    /// keeps the editor open with the reason in the panel's status line.
    fn rename_submit(&mut self) -> Vec<Effect> {
        let Some(rename) = self.file_ops.rename.take() else {
            return Vec::new();
        };
        match rename.submit() {
            Ok(None) => Vec::new(),
            Ok(Some((command, follow))) => vec![Effect::Ops(Submit {
                panel: rename.panel,
                command,
                follow,
                background: true,
            })],
            Err(code) => {
                let panel = rename.panel;
                self.pane_mut(panel).error = Some(error_text(code));
                self.file_ops.rename = Some(rename);
                vec![focus(RENAME_ID, None)]
            }
        }
    }

    /// Runs `command` for job `job`.
    fn steer(&self, job: JobId, command: Command) -> Vec<Effect> {
        let panel = self
            .file_ops
            .jobs
            .get(job)
            .map_or(self.active, |tracked| tracked.panel);
        vec![Effect::Dispatch(panel, vec![command])]
    }

    /// Answers the question job `job` waits on.
    fn answer(&self, job: JobId, answer: Answer) -> Vec<Effect> {
        self.steer(job, Command::OpsAnswer { job, answer })
    }

    /// "Rename to" in the conflict dialog: the edited name, checked here first so a typo keeps
    /// the dialog with its reason.
    fn rename_to(&mut self, job: JobId) -> Vec<Effect> {
        let Some(tracked) = self.file_ops.jobs.get_mut(job) else {
            return Vec::new();
        };
        let name = tracked.rename_to.trim().to_owned();
        if let Err(error) = check_name(&name) {
            tracked.note = Some(error.code());
            return vec![focus(PROPOSAL_ID, None)];
        }
        self.answer(job, Answer::RenameTo { name })
    }

    /// The modifiers held, for the function-key bar and a drop (Shift moves).
    pub(super) fn modifiers_held(&self) -> Chord {
        let modifiers = self.file_ops.modifiers;
        Chord {
            key: String::new(),
            ctrl: modifiers.control(),
            shift: modifiers.shift(),
            alt: modifiers.alt(),
        }
    }
}

#[cfg(test)]
#[path = "file_ops_tests.rs"]
mod tests;
