//! The checksum dialogs (RC-020-05), Total Commander's "Create Checksum File(s)", "Verify
//! Checksums", "Split File" and "Combine Files". Total Commander binds no key to them, only its
//! Files menu; rCommander has them in its Files menu too (RC-080-18) and gives each a key of its
//! own as well: `checksum.dialog` in the keymap, which the engine announces; the dialog opens
//! here for the panel's selection, else the entry under the cursor - a split or a combine for
//! one file.
//!
//! OK sends `checksum.queue`: the work is a job of the operations queue, and the file operations
//! follow it (`file_ops`) - the progress window with pause, background and cancel, a failure in
//! the panel's status line, the panels read again at the end. A check asks nothing and starts at
//! once, one job per selected checksum file. When such a job completes its report comes here
//! (`report.rs`): a check's lines and a create's digests go into the result list, a split's or a
//! combine's summary into the status bar. Drawing is `view.rs`, the part sizes `size.rs`, Enter
//! and a double click on a checksum file `enter.rs` (RC-020-10).

mod enter;
mod report;
mod size;
mod view;

pub(super) use self::enter::{concerns as enter_concerns, is_checksum_file};
pub(super) use self::view::{FIRST_FIELD, body, overlay, results_view, title};

use std::fmt;

use rc_engine::{
    Algorithm, ChecksumDialog, ChecksumFilesRequest, ChecksumJob, Chord, CombineRequest, Command,
    Entry, JobChange, JobId, JobState, MAX_PARTS, PanelId, PanelState, SplitRequest, plain_name,
};

use self::{
    report::{Section, Shown},
    size::{SizePreset, parse_size},
};
use super::{
    App, Effect,
    file_ops::{FollowUp, Submit},
    panel::Pane,
};
use crate::i18n::t;

/// What the checksum dialogs say.
#[derive(Debug, Clone)]
pub enum ChecksumMsg {
    Algorithm(AlgorithmChoice),
    /// Create: one checksum file beside each file.
    PerFile(bool),
    /// Create: the one checksum file's name; combine: the combined file's.
    Name(String),
    /// Split and combine: the target folder.
    Target(String),
    /// Split: the part size as typed.
    PartSize(String),
    Preset(SizePreset),
    /// `checksum.enter_verifies` as stored (RC-020-10).
    EnterVerifies(bool),
    Confirm,
    /// Escape, Cancel or the result list's Close.
    Close,
}

/// An algorithm as the pick list shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlgorithmChoice(pub Algorithm);

impl fmt::Display for AlgorithmChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.0 {
            Algorithm::Crc32 => t!("checksum.algorithm.crc32"),
            Algorithm::Md5 => t!("checksum.algorithm.md5"),
            Algorithm::Sha1 => t!("checksum.algorithm.sha1"),
            Algorithm::Sha256 => t!("checksum.algorithm.sha256"),
            Algorithm::Sha512 => t!("checksum.algorithm.sha512"),
            Algorithm::Blake3 => t!("checksum.algorithm.blake3"),
        })
    }
}

#[derive(Debug, Clone)]
pub struct CreateDialog {
    pub panel: PanelId,
    /// The entries of the panel's folder: files, and folders with every file in them.
    pub names: Vec<String>,
    pub algorithm: Algorithm,
    pub per_file: bool,
    /// The one checksum file's name, as the engine would choose it, then as edited.
    pub name: String,
}

impl CreateDialog {
    /// Whether the typed name will do: empty (the engine's choice) or one plain file name - no
    /// subfolder, also not with `\` on a Windows panel. The checksum file lies in the folder of
    /// the files it lists, which its lines name relative to it.
    pub fn name_ok(&self) -> bool {
        let name = self.name.trim();
        self.per_file || name.is_empty() || plain_name(name).is_ok()
    }
}

#[derive(Debug, Clone)]
pub struct SplitDialog {
    pub panel: PanelId,
    pub file: String,
    /// The file's size, for the number of parts.
    pub size: u64,
    pub target: String,
    pub part_size: String,
}

impl SplitDialog {
    /// The part size typed, and how many parts it makes; the reason when it will not do.
    pub fn parts(&self) -> Result<(u64, u64), &'static str> {
        let part = parse_size(&self.part_size).ok_or("split_part_size_invalid")?;
        let parts = self.size.div_ceil(part).max(1);
        if parts > MAX_PARTS {
            return Err("split_too_many_parts");
        }
        Ok((part, parts))
    }
}

#[derive(Debug, Clone)]
pub struct CombineDialog {
    pub panel: PanelId,
    /// The first part or the `.crc` file.
    pub file: String,
    pub target: String,
    /// Empty: as the `.crc` file says, else the parts' stem.
    pub name: String,
}

/// The open checksum window.
#[derive(Debug, Clone)]
pub enum ChecksumWindow {
    Create(CreateDialog),
    Split(SplitDialog),
    Combine(CombineDialog),
    /// The checks and creates that completed while it is open, oldest first.
    Results(Vec<Section>),
}

/// The one file a split or combine is for: the only selected entry, else the cursor's - none on
/// the `..` row.
fn one_file(pane: &Pane) -> Option<&Entry> {
    let state = &pane.state;
    let entry = match state.selection.len() {
        1 => state
            .entries
            .iter()
            .find(|entry| state.selection.contains(&entry.name)),
        _ if pane.on_parent() => None,
        _ => state.cursor_entry(),
    }?;
    (!entry.is_dir_like()).then_some(entry)
}

/// The name the engine gives the one checksum file: `<file>.<ext>` for one file, else
/// `checksums.<ext>`.
fn default_name(state: &PanelState, names: &[String], algorithm: Algorithm) -> String {
    let extension = algorithm.extension();
    let one_file = match names {
        [name] => state
            .entries
            .iter()
            .any(|entry| entry.name == *name && !entry.is_dir_like()),
        _ => false,
    };
    if one_file {
        format!("{}.{extension}", names[0])
    } else {
        format!("checksums.{extension}")
    }
}

fn job(panel: PanelId, job: ChecksumJob, follow: FollowUp, background: bool) -> Effect {
    Effect::Ops(Submit {
        panel,
        command: Command::QueueChecksumJob(job),
        follow,
        background,
    })
}

/// The check of the checksum file `path` of `panel`'s folder, as Ctrl+Shift+F10 and Enter start
/// it: in the progress window, or in the background bar.
fn verify_job(panel: PanelId, path: String, background: bool) -> Effect {
    let check = ChecksumJob::Verify {
        panel: Some(panel),
        path,
    };
    job(panel, check, FollowUp::Cursor(None), background)
}

impl App {
    /// `checksum.dialog` was dispatched: its dialog opens over the window - a check starts at
    /// once. Nothing opens over another dialog or where there is nothing to work on, as on the
    /// `..` row without a selection.
    pub(super) fn open_checksum_dialog(
        &mut self,
        panel: PanelId,
        dialog: ChecksumDialog,
    ) -> Vec<Effect> {
        let busy = matches!(
            self.checksum,
            Some(ChecksumWindow::Create(_) | ChecksumWindow::Split(_) | ChecksumWindow::Combine(_))
        );
        if busy || self.file_ops.modal() {
            return Vec::new();
        }
        let pane = self.pane(panel);
        let state = &pane.state;
        let other = self
            .pane(panel.other())
            .state
            .path
            .path
            .display()
            .to_string();
        let window = match dialog {
            ChecksumDialog::Create => {
                let names = pane.chosen_names();
                if names.is_empty() {
                    return Vec::new();
                }
                let algorithm = Algorithm::Sha256;
                ChecksumWindow::Create(CreateDialog {
                    panel,
                    name: default_name(state, &names, algorithm),
                    names,
                    algorithm,
                    per_file: false,
                })
            }
            ChecksumDialog::Verify => return self.verify(panel),
            ChecksumDialog::Split => {
                let Some(entry) = one_file(pane) else {
                    return Vec::new();
                };
                ChecksumWindow::Split(SplitDialog {
                    panel,
                    file: entry.name.clone(),
                    size: entry.size,
                    target: other,
                    part_size: SizePreset::Gib1.text(),
                })
            }
            ChecksumDialog::Combine => {
                let Some(entry) = one_file(pane) else {
                    return Vec::new();
                };
                ChecksumWindow::Combine(CombineDialog {
                    panel,
                    file: entry.name.clone(),
                    target: other,
                    name: String::new(),
                })
            }
        };
        self.checksum = Some(window);
        Vec::new()
    }

    /// "Verify checksums": a job for each chosen checksum file, the first in the progress
    /// window, the others in the background bar.
    fn verify(&self, panel: PanelId) -> Vec<Effect> {
        self.pane(panel)
            .chosen_names()
            .into_iter()
            .enumerate()
            .map(|(index, path)| verify_job(panel, path, index > 0))
            .collect()
    }

    /// A key while a checksum window is open: Escape closes it, Enter (one a field has not
    /// used already) confirms it, Tab and Shift+Tab walk the fields of a form.
    pub(super) fn checksum_key(&mut self, chord: &Chord, captured: bool) -> Vec<Effect> {
        if !captured && let Some(effects) = super::field_walk::walk_key(chord) {
            return effects;
        }
        if !chord.is_plain() {
            return Vec::new();
        }
        match chord.key.as_str() {
            "Escape" => self.on_checksum(ChecksumMsg::Close),
            "Enter" if !captured => self.on_checksum(ChecksumMsg::Confirm),
            _ => Vec::new(),
        }
    }

    /// A message of the checksum windows.
    pub(super) fn on_checksum(&mut self, message: ChecksumMsg) -> Vec<Effect> {
        match message {
            ChecksumMsg::Close => self.checksum = None,
            ChecksumMsg::Confirm => return self.confirm_checksum(),
            ChecksumMsg::EnterVerifies(on) => self.file_ops.enter_opens_checksums = !on,
            message => {
                if let Some(window) = self.checksum.as_mut() {
                    edit(window, message);
                }
            }
        }
        Vec::new()
    }

    /// OK: the dialog's job; the result list only closes. A part size or a checksum file name
    /// that will not do keeps the dialog, which shows why.
    fn confirm_checksum(&mut self) -> Vec<Effect> {
        if let Some(ChecksumWindow::Split(split)) = &self.checksum
            && split.parts().is_err()
        {
            return Vec::new();
        }
        if let Some(ChecksumWindow::Create(create)) = &self.checksum
            && !create.name_ok()
        {
            return Vec::new();
        }
        let effect = match self.checksum.take() {
            Some(ChecksumWindow::Create(create)) => {
                let name = create.name.trim().to_owned();
                let file_name = (!create.per_file && !name.is_empty()).then(|| name.clone());
                let follow = FollowUp::Cursor(file_name.clone());
                let request = ChecksumFilesRequest {
                    panel: Some(create.panel),
                    dir: None,
                    names: create.names,
                    algorithm: create.algorithm,
                    per_file: create.per_file,
                    file_name,
                };
                job(create.panel, ChecksumJob::Create(request), follow, false)
            }
            Some(ChecksumWindow::Split(split)) => {
                let Ok((part_size, _)) = split.parts() else {
                    return Vec::new();
                };
                let request = SplitRequest {
                    panel: Some(split.panel),
                    path: split.file,
                    part_size,
                    target_dir: target(&split.target),
                };
                job(
                    split.panel,
                    ChecksumJob::Split(request),
                    FollowUp::Refresh,
                    false,
                )
            }
            Some(ChecksumWindow::Combine(combine)) => {
                let name = combine.name.trim();
                let request = CombineRequest {
                    panel: Some(combine.panel),
                    path: combine.file,
                    target_dir: target(&combine.target),
                    target_name: (!name.is_empty()).then(|| name.to_owned()),
                };
                job(
                    combine.panel,
                    ChecksumJob::Combine(request),
                    FollowUp::Refresh,
                    false,
                )
            }
            Some(ChecksumWindow::Results(_)) | None => return Vec::new(),
        };
        vec![effect]
    }

    /// A job's end: a completed checksum job's report goes into the result list or the status
    /// bar. Over an open dialog the list does not open; its title goes to the status bar.
    pub(super) fn checksum_result(&mut self, job: JobId, change: &JobChange) {
        let JobChange::Finished {
            state: JobState::Completed,
        } = change
        else {
            return;
        };
        let Some(shown) = self
            .engine
            .ops()
            .job(job)
            .ok()
            .and_then(|snapshot| report::shown(&snapshot))
        else {
            return;
        };
        match shown {
            Shown::Status(text) => self.info = Some(text),
            Shown::List(section) => match &mut self.checksum {
                Some(ChecksumWindow::Results(sections)) => sections.push(section),
                Some(_) => self.info = Some(section.title),
                None => self.checksum = Some(ChecksumWindow::Results(vec![section])),
            },
        }
    }
}

/// A target folder as typed; empty means beside the file.
fn target(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// An edit in a dialog. A new algorithm changes the checksum file's ending with it.
fn edit(window: &mut ChecksumWindow, message: ChecksumMsg) {
    match (window, message) {
        (ChecksumWindow::Create(create), ChecksumMsg::Algorithm(AlgorithmChoice(algorithm))) => {
            let old = format!(".{}", create.algorithm.extension());
            if let Some(stem) = create.name.strip_suffix(&old) {
                create.name = format!("{stem}.{}", algorithm.extension());
            }
            create.algorithm = algorithm;
        }
        (ChecksumWindow::Create(create), ChecksumMsg::PerFile(on)) => create.per_file = on,
        (ChecksumWindow::Create(create), ChecksumMsg::Name(name)) => create.name = name,
        (ChecksumWindow::Split(split), ChecksumMsg::Target(target)) => split.target = target,
        (ChecksumWindow::Split(split), ChecksumMsg::PartSize(size)) => split.part_size = size,
        (ChecksumWindow::Split(split), ChecksumMsg::Preset(preset)) => {
            split.part_size = preset.text();
        }
        (ChecksumWindow::Combine(combine), ChecksumMsg::Target(target)) => combine.target = target,
        (ChecksumWindow::Combine(combine), ChecksumMsg::Name(name)) => combine.name = name,
        _ => {}
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
