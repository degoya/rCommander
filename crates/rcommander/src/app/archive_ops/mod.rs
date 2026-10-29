//! The archive dialogs (RC-030-03, RC-030-04): Alt+F5 packs, Alt+Shift+F5 packs and deletes the
//! sources, Alt+F6 and Alt+F9 unpack - each first in a dialog, as in Total Commander - and the
//! password an encrypted archive asks for while it unpacks.
//!
//! The keymap names `archive.pack` and `archive.extract` without a target; such a command from a
//! key opens its dialog here instead of running ([`App::open_archive_dialog`]). The dialog fixes
//! the sources (selection, else the entry under the cursor), suggests the target in the other
//! panel and submits the full command; the job runs in the operations queue and the window
//! follows it like a copy (progress, questions, failures, both panels read again). Before an
//! archive is replaced or the sources are deleted the dialog asks once more; whether the archive
//! exists is asked off the window's thread.
//!
//! The password prompt opens on the job's password question and answers it with
//! `archive.password`, one job at a time and over no other archive dialog (`password.rs`); the
//! typed text is a [`SecretString`] from the first key (wiped on drop, never in a `Debug` print
//! of a message) and becomes a [`SecretParam`](rc_engine::SecretParam) when it is sent.

mod password;
mod view;

pub(super) use self::view::{PASSWORD_FIELD, TARGET_FIELD, body, overlay, title};

use std::fmt;

use iced::Task;
use rc_engine::{
    Chord, Command, EngineEvent, ExtractSettings, JobChange, JobId, JobOptions, PackFormat,
    PackSettings, PanelId, Question, VfsPath, default_archive_name,
};
use rc_vault::SecretString;

use super::{
    App, Effect, Message,
    file_ops::{FollowUp, Preset, Submit},
};
use crate::i18n::t;

/// What the archive dialogs say.
#[derive(Debug, Clone)]
pub enum ArchiveMsg {
    /// Pack: the archive's path.
    Name(String),
    Format(FormatChoice),
    Level(u8),
    Option(PackOption, bool),
    /// Unpack: the target directory.
    Target(String),
    Mask(String),
    OwnFolder(bool),
    KeepPaths(bool),
    Conflict(Preset),
    /// The password field.
    Password(Typed),
    Confirm,
    /// Escape or the close button; the password prompt skips the archive.
    Close,
    /// Whether the archive to write exists already.
    Checked(bool),
}

/// A switch of the pack dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackOption {
    Solid,
    Recurse,
    KeepPaths,
    EachOwn,
    DeleteSources,
    StoreLinks,
}

/// A format as the pick list shows it: its ending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatChoice(pub PackFormat);

impl fmt::Display for FormatChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.extension())
    }
}

/// The presets in the unpack dialog's order.
pub const CONFLICT_PRESETS: [Preset; 5] = [
    Preset::Ask,
    Preset::Skip,
    Preset::Overwrite,
    Preset::OverwriteOlder,
    Preset::Rename,
];

/// A preset as the unpack dialog's list names it; the copy dialog has its own words.
impl fmt::Display for Preset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Ask => t!("archive_ops.conflict_ask"),
            Self::Skip => t!("archive_ops.conflict_skip"),
            Self::Overwrite => t!("archive_ops.conflict_overwrite"),
            Self::OverwriteOlder => t!("archive_ops.conflict_older"),
            Self::Rename => t!("archive_ops.conflict_rename"),
        })
    }
}

/// The text of the password field, kept as `quick_connect.rs` keeps its secrets: wiped on drop,
/// `Debug` never prints it.
pub type Typed = SecretString;

/// Where the pack dialog stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackStep {
    Editing,
    /// Asking whether the archive exists.
    Checking,
    /// The archive exists, or the sources go: asked once more.
    Confirm {
        exists: bool,
    },
}

#[derive(Debug, Clone)]
pub struct PackDialog {
    pub panel: PanelId,
    pub sources: Vec<String>,
    /// The archive (or with "each its own archive" the directory), as typed.
    pub name: String,
    pub settings: PackSettings,
    pub step: PackStep,
    /// The other panel's directory, which a relative name starts from.
    base: VfsPath,
}

#[derive(Debug, Clone)]
pub struct ExtractDialog {
    pub panel: PanelId,
    pub archives: Vec<String>,
    pub target: String,
    pub mask: String,
    pub own_folder: bool,
    pub keep_paths: bool,
    pub conflict: Preset,
}

#[derive(Debug, Clone)]
pub struct PasswordPrompt {
    pub job: JobId,
    /// The archive's file name.
    pub archive: String,
    /// The last password did not fit.
    pub wrong: bool,
    pub typed: Typed,
}

/// The open archive dialog.
#[derive(Debug, Clone)]
pub enum ArchiveDialog {
    Pack(PackDialog),
    Extract(ExtractDialog),
    Password(PasswordPrompt),
}

/// Which dialog is open, for choosing what a message does.
#[derive(Clone, Copy)]
enum Kind {
    Pack(PackStep),
    Extract,
    Password,
}

/// Whether `event` is an extraction asking for a password.
pub(super) fn concerns(event: &EngineEvent) -> bool {
    matches!(
        event,
        EngineEvent::Job {
            change: JobChange::Question {
                question: Question::Password { .. }
            },
            ..
        }
    )
}

/// Whether `dialog` has the archive dialog's window while the dialogs have theirs: packing and
/// unpacking; the password is a small question with a window of its own (RC-080-23,
/// `dialog_windows/questions.rs`).
pub(super) fn windowed(dialog: &ArchiveDialog) -> bool {
    !matches!(dialog, ArchiveDialog::Password(_))
}

fn wrap(message: ArchiveMsg) -> Message {
    Message::Archive(message)
}

/// The job of a confirmed dialog, followed by the window as a copy is: its progress, its
/// conflicts and failing files asked, what failed in the status line, both panels read again at
/// its end. Sent as a plain command it ran unseen - a question waited for nobody and the panels
/// never showed what it unpacked (Windows test 2026-10-18).
fn job(panel: PanelId, command: Command) -> Effect {
    Effect::Ops(Submit {
        panel,
        command,
        follow: FollowUp::Refresh,
        background: false,
    })
}

impl App {
    /// A key's `archive.pack` or `archive.extract` without a target: its dialog, for the
    /// selection or the entry under the cursor of `panel`. Nothing to pack or unpack - also the
    /// `..` row without a selection, as in Total Commander: nothing.
    pub(super) fn open_archive_dialog(&mut self, panel: PanelId, command: Command) -> Vec<Effect> {
        let names = self.pane(panel).chosen_names();
        let state = &self.pane(panel).state;
        if names.is_empty() {
            return Vec::new();
        }
        let other = self.pane(panel.other()).state.path.clone();
        self.archive = match command {
            Command::ArchivePack { settings, .. } => {
                let sources: Vec<VfsPath> =
                    names.iter().map(|name| state.path.join(name)).collect();
                let stem = default_archive_name(state, &sources);
                let name = if settings.each_own_archive {
                    other.path.display().to_string()
                } else {
                    other
                        .path
                        .join(format!("{stem}.{}", settings.format.extension()))
                        .display()
                        .to_string()
                };
                Some(ArchiveDialog::Pack(PackDialog {
                    panel,
                    sources: names,
                    name,
                    settings,
                    step: PackStep::Editing,
                    base: other,
                }))
            }
            Command::ArchiveExtract { settings, .. } => {
                Some(ArchiveDialog::Extract(ExtractDialog {
                    panel,
                    archives: names,
                    target: other.path.display().to_string(),
                    mask: settings.mask,
                    own_folder: settings.own_folder,
                    keep_paths: settings.keep_paths,
                    conflict: Preset::Ask,
                }))
            }
            _ => return Vec::new(),
        };
        Vec::new()
    }

    /// A key while a dialog is open: Enter confirms, Escape closes, Tab and Shift+Tab walk the
    /// fields; the fields take the rest.
    pub(super) fn archive_key(&mut self, chord: &Chord) -> Vec<Effect> {
        if let Some(effects) = super::field_walk::walk_key(chord) {
            return effects;
        }
        if !chord.is_plain() {
            return Vec::new();
        }
        match chord.key.as_str() {
            "Escape" => self.on_archive(ArchiveMsg::Close),
            "Enter" => self.on_archive(ArchiveMsg::Confirm),
            _ => Vec::new(),
        }
    }

    /// A message of the dialogs. Once the dialog closed, a password a job waits for is asked.
    pub(super) fn on_archive(&mut self, message: ArchiveMsg) -> Vec<Effect> {
        let kind = match &self.archive {
            None => return Vec::new(),
            Some(ArchiveDialog::Pack(pack)) => Kind::Pack(pack.step),
            Some(ArchiveDialog::Extract(_)) => Kind::Extract,
            Some(ArchiveDialog::Password(_)) => Kind::Password,
        };
        let effects = self.archive_step(kind, message);
        self.ask_waiting_password();
        effects
    }

    fn archive_step(&mut self, kind: Kind, message: ArchiveMsg) -> Vec<Effect> {
        match (kind, message) {
            (Kind::Password, ArchiveMsg::Confirm | ArchiveMsg::Close) => self.answer_password(),
            (Kind::Pack(PackStep::Confirm { .. } | PackStep::Checking), ArchiveMsg::Close) => {
                // "No": back to the dialog.
                if let Some(ArchiveDialog::Pack(pack)) = self.archive.as_mut() {
                    pack.step = PackStep::Editing;
                }
                Vec::new()
            }
            (_, ArchiveMsg::Close) => {
                self.archive = None;
                Vec::new()
            }
            (Kind::Pack(_), ArchiveMsg::Confirm) => self.confirm_pack(),
            (Kind::Pack(_), ArchiveMsg::Checked(exists)) => self.checked_pack(exists),
            (Kind::Extract, ArchiveMsg::Confirm) => self.confirm_extract(),
            (_, message) => {
                match self.archive.as_mut() {
                    Some(ArchiveDialog::Pack(pack)) if pack.step == PackStep::Editing => {
                        edit_pack(pack, message);
                    }
                    Some(ArchiveDialog::Extract(extract)) => edit_extract(extract, message),
                    Some(ArchiveDialog::Password(prompt)) => {
                        if let ArchiveMsg::Password(typed) = message {
                            prompt.typed = typed;
                        }
                    }
                    _ => {}
                }
                Vec::new()
            }
        }
    }

    /// OK in the pack dialog: first whether the archive exists, then (where it does, or the
    /// sources go) once more, then the job.
    fn confirm_pack(&mut self) -> Vec<Effect> {
        let Some(ArchiveDialog::Pack(pack)) = self.archive.as_mut() else {
            return Vec::new();
        };
        match pack.step {
            PackStep::Editing if pack.name.trim().is_empty() => Vec::new(),
            PackStep::Editing => {
                pack.step = PackStep::Checking;
                let path = pack.base.path.join(pack.name.trim());
                let probe = async move { tokio::fs::try_exists(path).await.unwrap_or(false) };
                let task = Task::perform(probe, |exists| wrap(ArchiveMsg::Checked(exists)));
                vec![Effect::Task(task)]
            }
            PackStep::Checking => Vec::new(),
            PackStep::Confirm { exists } => self.submit_pack(exists),
        }
    }

    fn checked_pack(&mut self, exists: bool) -> Vec<Effect> {
        let Some(ArchiveDialog::Pack(pack)) = self.archive.as_mut() else {
            return Vec::new();
        };
        if pack.step != PackStep::Checking {
            return Vec::new();
        }
        // "Each its own archive" names a directory, which exists by design.
        let exists = exists && !pack.settings.each_own_archive;
        if exists || pack.settings.delete_sources {
            pack.step = PackStep::Confirm { exists };
            return Vec::new();
        }
        self.submit_pack(false)
    }

    fn submit_pack(&mut self, overwrite: bool) -> Vec<Effect> {
        let Some(ArchiveDialog::Pack(pack)) = self.archive.take() else {
            return Vec::new();
        };
        let command = Command::ArchivePack {
            panel: Some(pack.panel),
            sources: pack.sources,
            target: Some(pack.name.trim().to_owned()),
            settings: PackSettings {
                overwrite,
                ..pack.settings
            },
            options: JobOptions::default(),
        };
        vec![job(pack.panel, command)]
    }

    fn confirm_extract(&mut self) -> Vec<Effect> {
        let Some(ArchiveDialog::Extract(extract)) = self.archive.take() else {
            return Vec::new();
        };
        let target = extract.target.trim();
        let command = Command::ArchiveExtract {
            panel: Some(extract.panel),
            archives: extract.archives,
            target: (!target.is_empty()).then(|| target.to_owned()),
            settings: ExtractSettings {
                own_folder: extract.own_folder,
                mask: extract.mask,
                keep_paths: extract.keep_paths,
            },
            options: JobOptions {
                on_conflict: extract.conflict.choice(),
                ..JobOptions::default()
            },
        };
        vec![job(extract.panel, command)]
    }
}

/// An edit in the pack dialog. A new format changes the archive's ending with it.
fn edit_pack(pack: &mut PackDialog, message: ArchiveMsg) {
    let settings = &mut pack.settings;
    match message {
        ArchiveMsg::Name(name) => pack.name = name,
        ArchiveMsg::Format(FormatChoice(format)) => {
            let old = format!(".{}", settings.format.extension());
            if !settings.each_own_archive
                && let Some(stem) = pack.name.strip_suffix(&old)
            {
                pack.name = format!("{stem}.{}", format.extension());
            }
            settings.format = format;
        }
        ArchiveMsg::Level(level) => settings.level = level.min(9),
        ArchiveMsg::Option(option, on) => match option {
            PackOption::Solid => settings.solid = on,
            PackOption::Recurse => settings.recurse = on,
            PackOption::KeepPaths => settings.keep_paths = on,
            PackOption::EachOwn => settings.each_own_archive = on,
            PackOption::DeleteSources => settings.delete_sources = on,
            PackOption::StoreLinks => settings.store_links = on,
        },
        _ => {}
    }
}

fn edit_extract(extract: &mut ExtractDialog, message: ArchiveMsg) {
    match message {
        ArchiveMsg::Target(target) => extract.target = target,
        ArchiveMsg::Mask(mask) => extract.mask = mask,
        ArchiveMsg::OwnFolder(on) => extract.own_folder = on,
        ArchiveMsg::KeepPaths(on) => extract.keep_paths = on,
        ArchiveMsg::Conflict(preset) => extract.conflict = preset,
        _ => {}
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
