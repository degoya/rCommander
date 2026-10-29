//! The dialogs' state and logic, without a window: what a dialog shows when it opens, and the
//! command it runs on OK. The dialogs are thin - they collect what the command needs; the
//! engine reads the target (`ops.copy_to`), the queue does the work.

use std::path::{MAIN_SEPARATOR, Path};

use rc_engine::{
    Command, ConflictChoice, CursorTarget, DEFAULT_LIST, DeleteMode, ErrorPolicy, ExternalProgram,
    FileDialog, JobOptions, PanelId, PanelState, QueueList, QueueListId, check_name,
};

use super::super::panel;

/// What to do once a job is over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FollowUp {
    /// Both panels are read again; `deselect`: a copy, whose sources that made it are
    /// deselected once it is over; else a move, whose sources are deselected when it starts.
    Transfer { deselect: bool },
    /// The panel is read again and its cursor goes to this entry: a new folder, a renamed file.
    Cursor(Option<String>),
    /// Both panels are read again.
    Refresh,
}

/// The answer given in advance to every conflict of a copy, a move or an unpack.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Preset {
    /// Each conflict asks.
    #[default]
    Ask,
    Overwrite,
    Skip,
    OverwriteOlder,
    Rename,
}

impl Preset {
    pub const ALL: [Self; 5] = [
        Self::Ask,
        Self::Overwrite,
        Self::Skip,
        Self::OverwriteOlder,
        Self::Rename,
    ];

    /// The choice every conflict gets; `None`: each asks.
    pub(in crate::app) fn choice(self) -> Option<ConflictChoice> {
        match self {
            Self::Ask => None,
            Self::Overwrite => Some(ConflictChoice::Overwrite),
            Self::Skip => Some(ConflictChoice::Skip),
            Self::OverwriteOlder => Some(ConflictChoice::OverwriteOlder),
            Self::Rename => Some(ConflictChoice::Rename),
        }
    }
}

/// An open dialog of F5, Shift+F5, F6, F7, F8 or Shift+F4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
    pub panel: PanelId,
    /// Never `Rename`: Shift+F6 edits in the list ([`InlineRename`]).
    pub kind: FileDialog,
    /// The target, the folder's or the file's name; unused for a delete.
    pub text: String,
    /// The names handed to the command, fixed when the dialog opens: a selection changed
    /// meanwhile (a click, an MCP call) does not change what OK works on (RC-080-44).
    sources: Vec<String>,
    /// How many entries the operation takes, and the name of the one when it is one.
    pub count: usize,
    pub single: Option<String>,
    pub preset: Preset,
    pub verify: bool,
    /// Keep time stamps and rights (RC-060-05); on when the dialog opens.
    pub keep_attributes: bool,
    /// Why OK did nothing, for the dialog to show.
    pub error: Option<&'static str>,
    /// The queue list F2 puts a copy or move into (RC-010-15): the one chosen last.
    pub queue: QueueListId,
    /// The queue's lists to choose from, as the dialog opened.
    pub lists: Vec<QueueList>,
}

/// The entries an operation of `state` takes: the selection, else the entry under the cursor
/// (none on the `..` row). The names to hand over, in the selection's order, and how many.
fn sources(state: &PanelState, on_parent: bool) -> Option<(Vec<String>, usize, Option<String>)> {
    match state.selection.len() {
        0 if on_parent => None,
        0 => {
            let name = state.cursor_entry()?.name.clone();
            Some((vec![name.clone()], 1, Some(name)))
        }
        1 => {
            let name = state.selection.iter().next().cloned();
            Some((name.iter().cloned().collect(), 1, name))
        }
        count => Some((state.selection.iter().cloned().collect(), count, None)),
    }
}

/// `location` (a path here, `sftp://<name>/<path>` on a server) as a target to copy into: with
/// a separator at its end, `/` on a server.
fn into(location: &str) -> String {
    let mut text = location.to_owned();
    let separator = if text.contains("://") {
        '/'
    } else {
        MAIN_SEPARATOR
    };
    if !text.ends_with(['/', separator]) {
        text.push(separator);
    }
    text
}

impl Dialog {
    /// The dialog `kind` for `panel`, whose state is `state` and whose other panel shows
    /// `other` (its location as text); `None` when it has nothing to work on (the cursor on
    /// `..`, nothing selected). F7 offers the name under the cursor, file or folder, as Total
    /// Commander does; Shift+F4 only a file's.
    pub fn open(
        kind: FileDialog,
        panel: PanelId,
        state: &PanelState,
        on_parent: bool,
        other: &str,
    ) -> Option<Self> {
        let cursor_entry = state.cursor_entry().filter(|_| !on_parent);
        let cursor_name = cursor_entry.map(|entry| entry.name.clone());
        let cursor_file = cursor_entry
            .filter(|entry| !entry.is_dir_like())
            .map(|entry| entry.name.clone());
        let (sources, count, single, text) = match kind {
            FileDialog::Copy | FileDialog::Move => {
                let (sources, count, single) = sources(state, on_parent)?;
                (sources, count, single, into(other))
            }
            FileDialog::Delete | FileDialog::DeletePermanent => {
                let (sources, count, single) = sources(state, on_parent)?;
                (sources, count, single, String::new())
            }
            // One entry: the only selected one, else the one under the cursor.
            FileDialog::CopyHere => {
                let name = match sources(state, on_parent)? {
                    (_, 1, Some(name)) => name,
                    _ if on_parent => return None,
                    _ => state.cursor_entry()?.name.clone(),
                };
                (vec![name.clone()], 1, Some(name.clone()), name)
            }
            FileDialog::Mkdir => (Vec::new(), 0, None, cursor_name.unwrap_or_default()),
            FileDialog::NewText => (Vec::new(), 0, None, cursor_file.unwrap_or_default()),
            FileDialog::Rename => return None,
        };
        Some(Self {
            panel,
            kind,
            text,
            sources,
            count,
            single,
            preset: Preset::default(),
            verify: false,
            keep_attributes: true,
            error: None,
            queue: DEFAULT_LIST,
            lists: Vec::new(),
        })
    }

    /// The queue's `lists` to choose from, `last` chosen when it is still one of them.
    pub fn offer_lists(&mut self, lists: Vec<QueueList>, last: QueueListId) {
        self.queue = if lists.iter().any(|list| list.id == last) {
            last
        } else {
            DEFAULT_LIST
        };
        self.lists = lists;
    }

    /// F2 ("Queue"): [`Dialog::submit`], a copy, move or delete into the chosen queue list. OK
    /// runs it at once instead, outside the queue's lists, as Total Commander does (owner,
    /// 2026-10-18).
    ///
    /// # Errors
    ///
    /// As [`Dialog::submit`].
    pub fn submit_queued(&self) -> Result<(Command, FollowUp), &'static str> {
        let (mut command, follow) = self.submit()?;
        match &mut command {
            Command::OpsCopyTo { queue, .. } | Command::OpsMoveTo { queue, .. } => {
                *queue = Some(self.queue);
            }
            Command::OpsDelete(request) => request.queue = Some(self.queue),
            _ => {}
        }
        Ok((command, follow))
    }

    /// The dialog for `names` of its panel instead of the selection: the entries a drag took
    /// (RC-080-01). Nothing changes without names.
    pub fn take_names(&mut self, names: Vec<String>) {
        if names.is_empty() {
            return;
        }
        self.count = names.len();
        self.single = match names.as_slice() {
            [name] => Some(name.clone()),
            _ => None,
        };
        self.sources = names;
    }

    /// Whether the dialog asks for text.
    pub fn has_field(&self) -> bool {
        !matches!(self.kind, FileDialog::Delete | FileDialog::DeletePermanent)
    }

    /// Whether the dialog starts a copy or a move (with options and the queue button).
    pub fn transfers(&self) -> bool {
        matches!(
            self.kind,
            FileDialog::Copy | FileDialog::CopyHere | FileDialog::Move
        )
    }

    /// Whether F2 ("Queue") puts its job into a queue list: a copy, a move, a delete.
    pub fn queues(&self) -> bool {
        self.transfers() || matches!(self.kind, FileDialog::Delete | FileDialog::DeletePermanent)
    }

    /// The characters to select when the field gets the keys: the name without its extension
    /// for Shift+F5, everything otherwise.
    pub fn selection(&self) -> Option<usize> {
        (self.kind == FileDialog::CopyHere).then(|| stem_len(&self.text))
    }

    /// The command OK runs and what follows its job; the error code when the text will not do.
    ///
    /// # Errors
    ///
    /// `invalid_name` for an empty target or name, or a new file's name that is no plain name.
    pub fn submit(&self) -> Result<(Command, FollowUp), &'static str> {
        let panel = Some(self.panel);
        let text = self.text.trim();
        if self.has_field() && text.is_empty() {
            return Err("invalid_name");
        }
        let options = JobOptions {
            on_conflict: self.preset.choice(),
            verify: self.verify,
            keep_attributes: self.keep_attributes,
            ..JobOptions::default()
        };
        let sources = self.sources.clone();
        Ok(match self.kind {
            FileDialog::Copy | FileDialog::CopyHere => (
                Command::OpsCopyTo {
                    panel,
                    sources,
                    target: text.to_owned(),
                    options,
                    queue: None,
                },
                FollowUp::Transfer { deselect: true },
            ),
            FileDialog::Move | FileDialog::Rename => (
                Command::OpsMoveTo {
                    panel,
                    sources,
                    target: text.to_owned(),
                    options,
                    queue: None,
                },
                FollowUp::Transfer { deselect: false },
            ),
            FileDialog::Mkdir => (
                Command::OpsMkdir {
                    panel,
                    path: text.to_owned(),
                },
                FollowUp::Cursor(first_part(text)),
            ),
            FileDialog::Delete | FileDialog::DeletePermanent => (
                Command::OpsDelete(rc_engine::DeleteRequest {
                    panel,
                    sources,
                    mode: if self.kind == FileDialog::Delete {
                        DeleteMode::Trash
                    } else {
                        DeleteMode::Permanent
                    },
                    on_error: ErrorPolicy::Ask,
                    queue: None,
                }),
                FollowUp::Refresh,
            ),
            FileDialog::NewText => {
                if Path::new(text).file_name().is_none() {
                    return Err("invalid_name");
                }
                (
                    Command::OpenExternal {
                        panel,
                        path: Some(text.to_owned()),
                        program: ExternalProgram::Editor,
                        create: true,
                    },
                    FollowUp::Cursor(first_part(text)),
                )
            }
        })
    }
}

/// The entry of the panel's directory that a relative `path` starts with: where the cursor goes
/// after `a/b/c` was made. `None` for an absolute path or one that leaves the directory.
pub fn first_part(path: &str) -> Option<String> {
    let path = Path::new(path);
    if path.is_absolute() {
        return None;
    }
    match path.components().next()? {
        std::path::Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
        _ => None,
    }
}

/// The characters of `name` before its extension: all of a directory-like or dot-file name.
fn stem_len(name: &str) -> usize {
    match name.rfind('.') {
        Some(at) if at > 0 => name[..at].chars().count(),
        _ => name.chars().count(),
    }
}

/// Shift+F6: the name of the entry under the cursor, edited in its row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineRename {
    pub panel: PanelId,
    /// The entry's index in the panel, to draw the editor in its row.
    pub index: usize,
    pub original: String,
    pub text: String,
    /// The characters selected at the start: the name without its extension.
    pub selected: usize,
}

impl InlineRename {
    /// The editor on the entry under the cursor; none on the `..` row.
    pub fn open(panel: PanelId, state: &PanelState, on_parent: bool) -> Option<Self> {
        if on_parent {
            return None;
        }
        let entry = state.cursor_entry()?;
        let selected = panel::stem(entry).chars().count();
        Some(Self {
            panel,
            index: state.cursor,
            original: entry.name.clone(),
            text: entry.name.clone(),
            selected,
        })
    }

    /// Enter: the move that renames, and the cursor on the new name; `Ok(None)` when the name
    /// did not change.
    ///
    /// # Errors
    ///
    /// The code of a name that is no plain file name.
    pub fn submit(&self) -> Result<Option<(Command, FollowUp)>, &'static str> {
        let name = self.text.trim();
        if name == self.original {
            return Ok(None);
        }
        check_name(name).map_err(|error| error.code())?;
        let rename = Command::OpsMoveTo {
            panel: Some(self.panel),
            sources: vec![self.original.clone()],
            target: name.to_owned(),
            options: JobOptions::default(),
            queue: None,
        };
        Ok(Some((rename, FollowUp::Cursor(Some(name.to_owned())))))
    }
}

/// The cursor's target after a follow-up.
pub fn cursor_on(name: &str) -> CursorTarget {
    CursorTarget::Name(name.to_owned())
}

#[cfg(test)]
#[path = "dialog_tests.rs"]
mod tests;
