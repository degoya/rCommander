//! The properties dialogs (RC-020-06): Alt+Enter shows an entry's properties with Total
//! Commander's "Change attributes" in the same card, Ctrl+L the occupied space of the selection,
//! Ctrl+Shift+F5 the link dialog, Ctrl+Z the comment of the entry under the cursor; Ctrl+Shift+C
//! copies the full paths of the selection to the clipboard, and Escape stops the size column's
//! count. This is rCommander's own dialog on every platform; the shell's comes with its context
//! menu (RC-080-02).
//!
//! The work is the engine's: each dialog runs one command off the window's thread
//! ([`PropsJob`]) and shows its outcome. Closing a dialog drops its running job - a size count
//! stops there. While a dialog is open it has the keys: Enter confirms, Escape closes.

mod form;
mod view;

use iced::{Task, task};
use rc_engine::{
    Chord, Command, DirSize, DirUsage, Engine, LinkKind, LinkKinds, Outcome, PanelId,
    PropertiesReport, VfsPath,
};

pub(super) use self::{
    form::AttributeForm,
    view::{body, overlay, title},
};
use super::{App, Effect, Message};
use crate::i18n::{error_text, t};

/// Which dialog is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    Properties,
    Space,
    Link,
    Comment,
}

/// Where a dialog stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// Its data is being read (or counted).
    Loading,
    Ready,
    /// A change runs.
    Running,
    Done(String),
    Failed(String),
}

/// The link dialog's fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkForm {
    pub target: VfsPath,
    /// Where the link goes, as typed.
    pub link: String,
    pub kind: LinkKind,
    /// What the file system can make; the others are greyed out.
    pub kinds: LinkKinds,
}

/// An open dialog.
#[derive(Debug)]
pub struct PropsDialog {
    pub kind: DialogKind,
    pub phase: Phase,
    panel: PanelId,
    /// The properties, once read.
    pub report: Option<PropertiesReport>,
    /// The attribute fields of the properties dialog.
    pub form: AttributeForm,
    /// The occupied space, once counted.
    pub space: Option<DirUsage>,
    pub link: Option<LinkForm>,
    /// The comment dialog's text.
    pub comment: String,
    /// The running job; dropping it aborts the job.
    task: Option<task::Handle>,
}

impl PropsDialog {
    fn new(kind: DialogKind, panel: PanelId, phase: Phase) -> Self {
        Self {
            kind,
            phase,
            panel,
            report: None,
            form: AttributeForm::default(),
            space: None,
            link: None,
            comment: String::new(),
            task: None,
        }
    }

    /// The entries the report is about, as absolute paths for the next command.
    fn paths(&self) -> Vec<String> {
        self.report.as_ref().map_or_else(Vec::new, |report| {
            report
                .items
                .iter()
                .map(|item| item.path.path.display().to_string())
                .collect()
        })
    }
}

/// A field of the dialogs, or a button.
#[derive(Debug, Clone)]
pub enum PropsMsg {
    /// The engine's answer: the outcome, or the code of the failure.
    Finished(Box<Result<Outcome, &'static str>>),
    Form(form::FormMsg),
    /// Count the folders' contents in the properties dialog.
    Count,
    LinkKind(LinkKind),
    LinkPath(String),
    Comment(String),
    Confirm,
    Close,
}

/// A command of a dialog for the engine.
pub struct PropsJob {
    command: Command,
}

impl PropsJob {
    /// Runs the job: off the window's thread in the app, inline in the tests.
    pub async fn run(self, engine: Engine) -> Result<Outcome, &'static str> {
        engine
            .dispatch(self.command)
            .await
            .map_err(|error| error.code())
    }

    /// The job as a task whose answer comes back to the dialogs, and the handle that stops it.
    pub fn into_task(self, engine: Engine) -> (Task<Message>, task::Handle) {
        Task::perform(self.run(engine), |result| {
            Message::Properties(PropsMsg::Finished(Box::new(result)))
        })
        .abortable()
    }
}

impl App {
    /// A command from a key that one of the dialogs handles instead of the engine directly;
    /// `None` for every other command.
    pub(super) fn properties_command(
        &mut self,
        panel: PanelId,
        command: &Command,
    ) -> Option<Vec<Effect>> {
        let job = |command| vec![Effect::Properties(PropsJob { command })];
        let (dialog, command) = match command {
            Command::GetProperties { .. } => (
                PropsDialog::new(DialogKind::Properties, panel, Phase::Loading),
                Command::GetProperties {
                    panel: Some(panel),
                    paths: Vec::new(),
                    count: false,
                },
            ),
            Command::DirSize { paths, .. } if paths.is_empty() => (
                PropsDialog::new(DialogKind::Space, panel, Phase::Loading),
                Command::DirSize {
                    panel: Some(panel),
                    paths: Vec::new(),
                },
            ),
            Command::CreateLink { link: None, .. } => {
                self.open_link_dialog(panel);
                return Some(Vec::new());
            }
            Command::SetComment { comment: None, .. } => (
                PropsDialog::new(DialogKind::Comment, panel, Phase::Loading),
                Command::GetProperties {
                    panel: Some(panel),
                    paths: self.cursor_path(panel).into_iter().collect(),
                    count: false,
                },
            ),
            Command::CopyNames { format, .. } => {
                return Some(job(Command::CopyNames {
                    panel: Some(panel),
                    format: *format,
                }));
            }
            _ => return None,
        };
        self.properties = Some(dialog);
        Some(job(command))
    }

    /// Escape while the active panel counts folder sizes stops the count.
    pub(super) fn counting_escape(&self, chord: &Chord) -> Option<Vec<Effect>> {
        let plain = chord.is_plain();
        let counting = self
            .pane(self.active)
            .state
            .dir_sizes
            .values()
            .any(|size| *size == DirSize::Counting);
        (plain && chord.key == "Escape" && counting).then(|| {
            let cancel = Command::CancelDirSizes {
                panel: Some(self.active),
            };
            vec![Effect::Dispatch(self.active, vec![cancel])]
        })
    }

    /// The entry under `panel`'s cursor as an absolute path; none on an empty panel.
    fn cursor_path(&self, panel: PanelId) -> Option<String> {
        let state = &self.pane(panel).state;
        state
            .cursor_entry()
            .map(|entry| state.path.path.join(&entry.name).display().to_string())
    }

    /// The link dialog for the entry under the cursor, the link proposed in the other panel's
    /// folder under the same name - with `.lnk` for a shortcut, the default where possible.
    fn open_link_dialog(&mut self, panel: PanelId) {
        let state = &self.pane(panel).state;
        let Some(entry) = state.cursor_entry() else {
            return;
        };
        let target = state.path.join(&entry.name);
        let kinds = self.engine.property_capabilities(&target).links;
        let kind = if kinds.shortcut {
            LinkKind::Shortcut
        } else {
            LinkKind::Symlink
        };
        let other = &self.pane(panel.other()).state.path;
        let link = link_name(&other.join(&entry.name), kind);
        let mut dialog = PropsDialog::new(DialogKind::Link, panel, Phase::Ready);
        dialog.link = Some(LinkForm {
            target,
            link,
            kind,
            kinds,
        });
        self.properties = Some(dialog);
    }

    /// A message of the dialogs.
    pub(super) fn on_properties(&mut self, message: PropsMsg) -> Vec<Effect> {
        match message {
            PropsMsg::Finished(result) => return self.properties_finished(*result),
            PropsMsg::Close => self.properties = None,
            PropsMsg::Confirm => return self.confirm_properties(),
            PropsMsg::Count => {
                if let Some(dialog) = self.editable_dialog() {
                    dialog.phase = Phase::Loading;
                    let panel = dialog.panel;
                    let paths = dialog.paths();
                    let command = Command::GetProperties {
                        panel: Some(panel),
                        paths,
                        count: true,
                    };
                    return vec![Effect::Properties(PropsJob { command })];
                }
            }
            PropsMsg::Form(message) => {
                if let Some(dialog) = self.editable_dialog() {
                    dialog.form.update(message);
                    dialog.phase = Phase::Ready;
                }
            }
            PropsMsg::LinkKind(kind) => {
                if let Some(link) = self
                    .editable_dialog()
                    .and_then(|dialog| dialog.link.as_mut())
                    && link.kinds.supports(kind)
                {
                    let shortcut_before = link.kind == LinkKind::Shortcut;
                    link.kind = kind;
                    if shortcut_before != (kind == LinkKind::Shortcut) {
                        link.link = relinked(&link.link, kind);
                    }
                }
            }
            PropsMsg::LinkPath(path) => {
                if let Some(link) = self
                    .editable_dialog()
                    .and_then(|dialog| dialog.link.as_mut())
                {
                    link.link = path;
                }
            }
            PropsMsg::Comment(text) => {
                if let Some(dialog) = self.editable_dialog() {
                    dialog.comment = text;
                }
            }
        }
        Vec::new()
    }

    /// A key while a dialog is open: Enter confirms (or closes a finished one), Escape closes,
    /// Tab and Shift+Tab walk the fields; the fields take the rest.
    pub(super) fn properties_key(&mut self, chord: &Chord) -> Vec<Effect> {
        if let Some(effects) = super::field_walk::walk_key(chord) {
            return effects;
        }
        if !chord.is_plain() {
            return Vec::new();
        }
        match chord.key.as_str() {
            "Escape" => self.properties = None,
            "Enter" => return self.confirm_properties(),
            _ => {}
        }
        Vec::new()
    }

    /// The open dialog unless it loads or runs.
    fn editable_dialog(&mut self) -> Option<&mut PropsDialog> {
        self.properties
            .as_mut()
            .filter(|dialog| !matches!(dialog.phase, Phase::Loading | Phase::Running))
    }

    /// Enter or the confirm button: apply the attributes, create the link, save the comment;
    /// a dialog that only shows closes.
    fn confirm_properties(&mut self) -> Vec<Effect> {
        let Some(dialog) = self.editable_dialog() else {
            return Vec::new();
        };
        let panel = Some(dialog.panel);
        let command = match dialog.kind {
            DialogKind::Space => None,
            _ if matches!(dialog.phase, Phase::Done(_)) => None,
            DialogKind::Properties => match dialog.form.change() {
                Ok(change) if change.is_empty() => None,
                Ok(change) => Some(Command::SetFileAttributes {
                    panel,
                    paths: dialog.paths(),
                    change,
                    recursive: dialog.form.recursive,
                    apply_to: dialog.form.apply_to,
                }),
                Err(problem) => {
                    dialog.phase = Phase::Failed(problem);
                    return Vec::new();
                }
            },
            DialogKind::Link => dialog.link.as_ref().map(|link| Command::CreateLink {
                panel,
                target: Some(link.target.path.display().to_string()),
                link: Some(link.link.trim().to_owned()),
                kind: Some(link.kind),
            }),
            DialogKind::Comment => Some(Command::SetComment {
                panel,
                path: dialog.paths().into_iter().next(),
                comment: Some(dialog.comment.clone()),
            }),
        };
        match command {
            Some(command) => {
                dialog.phase = Phase::Running;
                vec![Effect::Properties(PropsJob { command })]
            }
            None => {
                self.properties = None;
                Vec::new()
            }
        }
    }

    /// The engine's answer, for the open dialog or the clipboard.
    fn properties_finished(&mut self, result: Result<Outcome, &'static str>) -> Vec<Effect> {
        if let Ok(Outcome::ClipboardText { text }) = result {
            self.info = Some(t!("props.copied", lines = text.lines().count()));
            return vec![Effect::Task(iced::clipboard::write(text))];
        }
        let Some(dialog) = self.properties.as_mut() else {
            // A copy without anything to copy: the panel says so.
            if let Err(code) = result {
                self.pane_mut(self.active).error = Some(error_text(code));
            }
            return Vec::new();
        };
        dialog.task = None;
        match result {
            Ok(Outcome::Properties(report)) => {
                if let Some(first) = report.items.first() {
                    dialog.comment = first.comment.clone().unwrap_or_default();
                    dialog.form = AttributeForm::from_report(&report);
                }
                dialog.report = Some(report);
                dialog.phase = Phase::Ready;
            }
            Ok(Outcome::DirSizes { total, .. }) => {
                dialog.space = Some(total);
                dialog.phase = Phase::Done(String::new());
            }
            Ok(Outcome::AttributesSet(report)) if report.failed > 0 => {
                dialog.phase = Phase::Failed(t!(
                    "props.attributes_partly",
                    changed = report.changed,
                    failed = report.failed
                ));
            }
            Ok(Outcome::AttributesSet(report)) => {
                self.info = Some(t!("props.attributes_done", changed = report.changed));
                self.properties = None;
            }
            Ok(Outcome::LinkCreated { link, .. }) => {
                self.info = Some(t!("props.link_done", link = link.path.display()));
                self.properties = None;
            }
            Ok(Outcome::CommentSet { .. }) => {
                self.info = Some(t!("props.comment_done").to_owned());
                self.properties = None;
            }
            // The dialogs' commands answer nothing else.
            Ok(_) => dialog.phase = Phase::Ready,
            Err(code) => dialog.phase = Phase::Failed(error_text(code)),
        }
        Vec::new()
    }

    /// The task of a dialog's job; the open dialog keeps its handle, so closing it aborts.
    pub(super) fn properties_task(&mut self, job: PropsJob) -> Task<Message> {
        let (task, handle) = job.into_task(self.engine.clone());
        if let Some(dialog) = self.properties.as_mut() {
            dialog.task = Some(handle.abort_on_drop());
        }
        task
    }
}

/// The proposed link for `path`: its own name, with `.lnk` for a shortcut.
fn link_name(path: &VfsPath, kind: LinkKind) -> String {
    let shown = path.path.display().to_string();
    if kind == LinkKind::Shortcut {
        format!("{shown}.lnk")
    } else {
        shown
    }
}

/// The typed link after switching to or from a shortcut: `.lnk` added or taken off.
fn relinked(link: &str, kind: LinkKind) -> String {
    match (kind, link.strip_suffix(".lnk")) {
        (LinkKind::Shortcut, _) => format!("{link}.lnk"),
        (_, Some(stem)) => stem.to_owned(),
        (_, None) => link.to_owned(),
    }
}

#[cfg(test)]
#[path = "properties_tests.rs"]
mod tests;
