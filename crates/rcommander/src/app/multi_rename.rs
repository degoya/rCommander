//! The multi-rename dialog (Total Commander's Ctrl+M, RC-020-02): the masks with their
//! placeholder buttons, search and replace, case and counter on top, the preview table below,
//! the presets and the log beside.
//!
//! Ctrl+M is `multi_rename.open_dialog`; the engine announces it and the dialog opens over the
//! window on the panel's selection in panel order, or the entry under the cursor. The dialog
//! keeps no rename logic: every input becomes a [`rc_engine::RenameRule`] (`form.rs`) and the
//! engine previews it (`multi_rename.preview`) off the window's thread, debounced, the last
//! input winning (`call.rs`); the table (`list.rs`) shows only the rows on screen. Start runs
//! `multi_rename.apply` - only without a conflict - and hands the job to the operations' bar
//! (`file_ops`); when it is over the panel selects the renamed entries, the table shows their
//! new names and the optional log gets the pairs carried out (`run.rs`). Undo runs
//! `multi_rename.undo`. The presets are the engine's (`multi_rename.preset_*`).
//!
//! While the dialog is open it has the keyboard and the mouse (`keys.rs`); jobs keep running.

mod call;
mod controls;
mod form;
mod keys;
mod list;
mod run;
mod view;

use std::{
    fmt,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use rc_engine::{Command, EngineEvent, JobId, PanelId, RenamePreset, RenameRule, VfsPath};

pub(super) use self::view::{body, overlay};
pub use self::{
    call::{Answer, Call, PreviewError, Queued},
    form::{CaseChoice, Field, Flag, Form, Placeholder},
    list::RenameList,
};
use super::{App, Effect, Message};
use crate::i18n::{error_text, t};

/// A choice of the presets' drop-down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetChoice {
    /// Total Commander's `<Default>`: every field reset.
    Default,
    /// The rule of the last run.
    Last,
    Named(String),
}

impl fmt::Display for PresetChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => formatter.write_str(t!("multi_rename.preset_default")),
            Self::Last => formatter.write_str(t!("multi_rename.preset_last")),
            Self::Named(name) => formatter.write_str(name),
        }
    }
}

/// Where the preview stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Preview {
    /// Asked for, not answered yet.
    Pending,
    Ready {
        changes: usize,
        conflicts: usize,
    },
    /// The rule does not work: a mask, a regular expression, a missing entry.
    Failed(PreviewError),
    /// A counter field is no number.
    Invalid(Field),
}

/// The status line: what the dialog last did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    None,
    Busy,
    Info(String),
    Error(String),
}

/// A run or an undo of this dialog while its job runs.
#[derive(Debug, Clone)]
struct Running {
    job: JobId,
    undo: bool,
    run: String,
    dir: VfsPath,
    /// The pairs `old -> new` of a run, for its log and the names after it.
    pairs: Vec<(String, String)>,
    /// The names before it.
    before: Vec<String>,
}

/// The open dialog, or the closed one whose run has not ended yet.
#[derive(Debug)]
pub struct MultiRenameDialog {
    pub open: bool,
    pub panel: PanelId,
    /// The folder of the entries.
    pub dir: VfsPath,
    pub form: Form,
    pub list: RenameList,
    pub preview: Preview,
    pub status: Status,
    pub presets: Vec<RenamePreset>,
    pub last: Option<RenameRule>,
    pub preset: PresetChoice,
    pub preset_name: String,
    pub log: bool,
    pub log_path: String,
    /// The panel had a selection, which the renamed entries keep.
    selected: bool,
    /// A run or an undo was asked for and the engine has not answered yet.
    asked: bool,
    running: Option<Running>,
    /// The pairs of the run asked for.
    pending_pairs: Vec<(String, String)>,
    /// The names before this dialog's last run, which its undo brings back.
    undo_names: Option<Vec<String>>,
    /// The number of the latest preview asked for, shared with the waiting previews.
    generation: u64,
    latest: Arc<AtomicU64>,
}

impl MultiRenameDialog {
    pub fn new(panel: PanelId, dir: VfsPath, names: Vec<String>, selected: bool) -> Self {
        Self {
            open: true,
            panel,
            dir,
            form: Form::default(),
            list: RenameList::new(names),
            preview: Preview::Pending,
            status: Status::None,
            presets: Vec::new(),
            last: None,
            preset: PresetChoice::Default,
            preset_name: String::new(),
            log: false,
            log_path: String::new(),
            selected,
            asked: false,
            running: None,
            pending_pairs: Vec::new(),
            undo_names: None,
            generation: 0,
            latest: Arc::new(AtomicU64::new(0)),
        }
    }

    /// A run or an undo is asked for or running.
    pub fn busy(&self) -> bool {
        self.asked || self.running.is_some()
    }

    /// Whether the log is set up as it must be: off, or an absolute path.
    pub fn log_ready(&self) -> bool {
        !self.log || Path::new(self.log_path.trim()).is_absolute()
    }

    /// Start is offered: a preview of the input as it is, with changes and no conflict.
    pub fn can_start(&self) -> bool {
        !self.busy()
            && self.log_ready()
            && matches!(self.preview, Preview::Ready { changes, conflicts: 0 } if changes > 0)
    }

    /// The request the input makes, or the counter field that does not read.
    fn request(&self) -> Result<rc_engine::MultiRenameRequest, Field> {
        Ok(rc_engine::MultiRenameRequest {
            panel: Some(self.panel),
            dir: Some(self.dir.path.display().to_string()),
            names: self.list.names().to_vec(),
            rule: self.form.rule()?,
        })
    }

    /// A new preview of the input; the earlier ones are stale from now on.
    fn refresh(&mut self, debounce: bool) -> Option<Call> {
        self.generation = self.latest.fetch_add(1, Ordering::AcqRel) + 1;
        match self.request() {
            Ok(request) => {
                self.preview = Preview::Pending;
                Some(Call::Preview {
                    generation: self.generation,
                    latest: Arc::clone(&self.latest),
                    debounce,
                    request,
                })
            }
            Err(field) => {
                self.preview = Preview::Invalid(field);
                None
            }
        }
    }

    /// Every field as the preset has it.
    fn load(&mut self, choice: PresetChoice) {
        self.form = match &choice {
            PresetChoice::Default => Form::default(),
            PresetChoice::Last => self.last.as_ref().map(Form::from_rule).unwrap_or_default(),
            PresetChoice::Named(name) => self
                .presets
                .iter()
                .find(|preset| preset.name == *name)
                .map(|preset| Form::from_rule(&preset.rule))
                .unwrap_or_default(),
        };
        if let PresetChoice::Named(name) = &choice {
            self.preset_name.clone_from(name);
        }
        self.preset = choice;
    }

    /// F2: the input saved under the preset name.
    fn save_preset(&mut self) -> Vec<Effect> {
        let name = self.preset_name.trim().to_owned();
        if name.is_empty() {
            self.status = Status::Error(t!("multi_rename.preset_name_needed").to_owned());
            return vec![keys::focus(Field::PresetName)];
        }
        match self.form.rule() {
            Ok(rule) => {
                self.preset = PresetChoice::Named(name.clone());
                let save = Command::MultiRenamePresetSave { name, rule };
                vec![Effect::MultiRename(Call::Presets(save))]
            }
            Err(field) => {
                self.preview = Preview::Invalid(field);
                Vec::new()
            }
        }
    }

    /// The chosen preset gets the preset name typed.
    fn rename_preset(&mut self) -> Vec<Effect> {
        let new_name = self.preset_name.trim().to_owned();
        let PresetChoice::Named(name) = &self.preset else {
            return Vec::new();
        };
        if new_name.is_empty() || new_name == *name {
            return Vec::new();
        }
        let rename = Command::MultiRenamePresetRename {
            name: name.clone(),
            new_name: new_name.clone(),
        };
        self.preset = PresetChoice::Named(new_name);
        vec![Effect::MultiRename(Call::Presets(rename))]
    }

    /// The chosen preset is deleted; the fields stay as they are.
    fn delete_preset(&mut self) -> Vec<Effect> {
        let PresetChoice::Named(name) = &self.preset else {
            return Vec::new();
        };
        let delete = Command::MultiRenamePresetDelete { name: name.clone() };
        self.preset = PresetChoice::Default;
        vec![Effect::MultiRename(Call::Presets(delete))]
    }
}

/// What the dialog's controls and its calls say.
#[derive(Debug, Clone)]
pub enum MrMsg {
    Text(Field, String),
    Flag(Flag, bool),
    Case(CaseChoice),
    Insert(Placeholder),
    Preset(PresetChoice),
    SavePreset,
    RenamePreset,
    DeletePreset,
    Log(bool),
    /// A row clicked, by its index.
    Click(usize),
    /// The wheel over the table, in rows.
    Scroll(isize),
    /// The table is this many rows high.
    Viewport(usize),
    SortByName,
    MoveUp,
    MoveDown,
    Start,
    Undo,
    Close,
    Answer(Answer),
}

impl App {
    /// While the dialog is open, keys and clicks belong to it; Ctrl+M opens it. `None` for a
    /// message the dialog does not take.
    pub(super) fn multi_rename_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        if let Message::Engine(EngineEvent::MultiRenameDialog { panel }) = message {
            return Some(self.open_multi_rename(*panel));
        }
        self.multi_rename.as_ref().filter(|dialog| dialog.open)?;
        match message {
            // Tab walks the focus ring; a key a stop of it took is not the dialog's again.
            Message::Key(chord) if super::field_walk::tab(chord).is_some() => {
                super::field_walk::walk_key(chord)
            }
            Message::KeyCaptured(chord) if super::field_walk::taken(chord) => Some(Vec::new()),
            Message::Key(chord) | Message::KeyCaptured(chord) => Some(self.multi_rename_key(chord)),
            Message::Press => Some(Vec::new()),
            _ => None,
        }
    }

    /// Opens the dialog on `panel`'s selection in panel order, else the entry under the cursor -
    /// on the `..` row nothing; a dialog whose run has not ended is shown again.
    fn open_multi_rename(&mut self, panel: PanelId) -> Vec<Effect> {
        if let Some(dialog) = self.multi_rename.as_mut().filter(|dialog| dialog.busy()) {
            dialog.open = true;
            return Vec::new();
        }
        let pane = self.pane(panel);
        let (names, selected) = (pane.chosen_names(), !pane.state.selection.is_empty());
        let dir = pane.state.path.clone();
        if names.is_empty() {
            self.pane_mut(panel).error = Some(t!("multi_rename.nothing").to_owned());
            return Vec::new();
        }
        self.multi_rename = Some(MultiRenameDialog::new(panel, dir, names, selected));
        let mut effects = vec![
            Effect::MultiRename(Call::Presets(Command::MultiRenamePresetList)),
            keys::focus(Field::NameMask),
        ];
        effects.extend(self.mr_refresh(false));
        effects
    }

    pub(super) fn on_multi_rename(&mut self, message: MrMsg) -> Vec<Effect> {
        let Some(dialog) = self.multi_rename.as_mut() else {
            return Vec::new();
        };
        match message {
            MrMsg::Text(field, text) => match field {
                Field::PresetName => dialog.preset_name = text,
                Field::LogPath => dialog.log_path = text,
                _ => {
                    if let Some(slot) = dialog.form.text_mut(field) {
                        *slot = text;
                    }
                    return self.mr_refresh(true);
                }
            },
            MrMsg::Flag(flag, on) => {
                *dialog.form.flag_mut(flag) = on;
                return self.mr_refresh(true);
            }
            MrMsg::Case(case) => {
                dialog.form.case = case;
                return self.mr_refresh(false);
            }
            MrMsg::Insert(placeholder) => {
                dialog.form.insert(placeholder);
                return self.mr_refresh(false);
            }
            MrMsg::Preset(choice) => {
                dialog.load(choice);
                return self.mr_refresh(false);
            }
            MrMsg::SavePreset => return dialog.save_preset(),
            MrMsg::RenamePreset => return dialog.rename_preset(),
            MrMsg::DeletePreset => return dialog.delete_preset(),
            MrMsg::Log(on) => dialog.log = on,
            MrMsg::Click(index) => dialog.list.point_at(index),
            MrMsg::Scroll(rows) => dialog.list.scroll(rows),
            MrMsg::Viewport(rows) => dialog.list.set_page(rows),
            MrMsg::SortByName => {
                dialog.list.sort_by_name();
                return self.mr_refresh(false);
            }
            MrMsg::MoveUp => {
                if dialog.list.move_row(-1) {
                    return self.mr_refresh(false);
                }
            }
            MrMsg::MoveDown => {
                if dialog.list.move_row(1) {
                    return self.mr_refresh(false);
                }
            }
            MrMsg::Start => return self.mr_start(),
            MrMsg::Undo => return self.mr_undo(),
            MrMsg::Close => self.close_multi_rename(),
            MrMsg::Answer(answer) => return self.mr_answer(answer),
        }
        Vec::new()
    }

    /// A new preview of the input, unless the panel has gone to another file system since the
    /// dialog opened (an agent's navigate): the folder's path would name another folder there.
    fn mr_refresh(&mut self, debounce: bool) -> Vec<Effect> {
        let Some(dialog) = self.multi_rename.as_mut() else {
            return Vec::new();
        };
        if self.panes[dialog.panel.index()].state.path.backend != dialog.dir.backend {
            dialog.preview = Preview::Failed(PreviewError {
                text: t!("multi_rename.panel_moved").to_owned(),
                mask: None,
            });
            return Vec::new();
        }
        dialog
            .refresh(debounce)
            .map(Effect::MultiRename)
            .into_iter()
            .collect()
    }

    /// Escape or Close: the dialog goes, without a change; one whose run has not ended stays
    /// hidden until it has.
    fn close_multi_rename(&mut self) {
        match self.multi_rename.as_mut() {
            Some(dialog) if dialog.busy() => dialog.open = false,
            _ => self.multi_rename = None,
        }
    }
}

/// A preset command's error, a run's, a log's: the status line's text.
fn failure(code: &str) -> Status {
    Status::Error(error_text(code))
}

#[cfg(test)]
#[path = "multi_rename_tests.rs"]
mod tests;
