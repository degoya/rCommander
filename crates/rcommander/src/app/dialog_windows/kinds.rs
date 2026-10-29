//! What each dialog's window shows (RC-080-15, RC-080-23): which dialogs have a window of their
//! own now ([`App::wanted_windows`]), what a dialog's window draws ([`App::dialog_layers`]), its
//! title and the field that takes the keyboard when it opens ([`first_field`]).

use iced::Element;

use super::DialogWindow;
use crate::{
    app::{
        App, Message, archive_ops,
        checksum::{self, ChecksumWindow},
        connections, database_dialog, drive_bar, file_ops, multi_rename, properties, quick_connect,
        search, settings, ssh_exec, tabs,
    },
    i18n::t,
    theme::{Tokens, layout},
};

impl App {
    /// The dialogs that have a window of their own now, the modal ones first - the file
    /// operations' windows (`dialog_windows/file_ops.rs`), a job's question on top -: the
    /// settings while open, the queue while open and `queue.window_own` is on (RC-010-14), and
    /// while `appearance.dialog_windows` is on multi-rename and the standalone dialogs
    /// (RC-080-23) - the database dialog, the properties, the quick connection, a network drive
    /// dialog,
    /// packing or unpacking (not the archive's password, a small question), a checksum form,
    /// the hotlist dialog, "Run command" -, the connection manager - not the unlock prompt a
    /// connection asked for alone, nor the Total Commander import the open settings show over
    /// themselves -, find files, compare/synchronize, the checksum results, the login-script
    /// editor and each Lister while open; last the top small question asked (`questions.rs`).
    pub(in crate::app) fn wanted_windows(&self) -> Vec<DialogWindow> {
        let books = &self.dialog_windows;
        let own = books.own;
        let manager = self.connections.as_ref().is_some_and(|manager| {
            !manager.prompt_only && !(manager.over_settings() && self.settings.is_some())
        });
        let archive = self.archive.as_ref().is_some_and(archive_ops::windowed);
        let checksum_form = self
            .checksum
            .as_ref()
            .is_some_and(|open| !matches!(open, ChecksumWindow::Results(_)));
        let open = [
            (DialogWindow::Settings, self.settings.is_some()),
            (
                DialogWindow::MultiRename,
                own && self.multi_rename.as_ref().is_some_and(|dialog| dialog.open),
            ),
            (DialogWindow::Database, own && self.database.is_some()),
            (DialogWindow::Properties, own && self.properties.is_some()),
            (DialogWindow::QuickConnect, own && self.quick.is_some()),
            (
                DialogWindow::Network,
                own && self.drive_bar.dialog.is_some(),
            ),
            (DialogWindow::Archive, own && archive),
            (DialogWindow::ChecksumForm, own && checksum_form),
            (DialogWindow::Hotlist, own && self.tabs.editor.is_some()),
            (DialogWindow::SshExec, own && self.ssh_exec.is_some()),
            (
                DialogWindow::Queue,
                books.queue_own && self.queue_window.is_some(),
            ),
            (DialogWindow::Connections, own && manager),
            (
                DialogWindow::Search,
                own && self.search.as_ref().is_some_and(|dialog| dialog.open),
            ),
            (
                DialogWindow::Compare,
                own && (self.compare.diff.is_some() || self.compare.sync.is_some()),
            ),
            (
                DialogWindow::Checksum,
                own && matches!(self.checksum, Some(ChecksumWindow::Results(_))),
            ),
            (
                DialogWindow::LoginScripts,
                own && self.login_scripts.is_some(),
            ),
        ];
        let open = open
            .into_iter()
            .filter_map(|(kind, open)| open.then_some(kind));
        self.file_ops_windows()
            .into_iter()
            .chain(open)
            .chain(self.lister_windows().filter(|_| own))
            .chain(self.small_question().map(DialogWindow::Question))
            .collect()
    }

    /// A dialog's window: the dialog on the card's surface, the system's title bar its head;
    /// nothing once the dialog closed.
    pub(super) fn dialog_layers(
        &self,
        kind: DialogWindow,
        tokens: &'static Tokens,
    ) -> Vec<Element<'_, Message>> {
        let body = match kind {
            // The settings, and the Total Commander import they opened (RC-050-10).
            DialogWindow::Settings => {
                return match &self.settings {
                    Some(dialog) => std::iter::once(layout::window_body(
                        dialog.view(self.settings_tabs, tokens),
                        tokens,
                    ))
                    .chain(
                        self.connections
                            .as_ref()
                            .filter(|manager| manager.over_settings())
                            .map(|manager| connections::layer(manager, tokens)),
                    )
                    .collect(),
                    None => Vec::new(),
                };
            }
            DialogWindow::Connections => self
                .connections
                .as_ref()
                .map(|manager| connections::body(manager, tokens)),
            DialogWindow::Search => self
                .search
                .as_ref()
                .filter(|dialog| dialog.open)
                .map(|dialog| search::body(dialog, tokens)),
            DialogWindow::MultiRename => self
                .multi_rename
                .as_ref()
                .filter(|dialog| dialog.open)
                .map(|dialog| multi_rename::body(dialog, tokens)),
            DialogWindow::Compare => return self.compare_window(tokens),
            DialogWindow::Checksum => match &self.checksum {
                Some(ChecksumWindow::Results(sections)) => {
                    Some(checksum::results_view(sections, tokens))
                }
                _ => None,
            },
            DialogWindow::LoginScripts => self.login_scripts_body(),
            DialogWindow::Lister(id) => return self.lister_body(id, tokens).into_iter().collect(),
            DialogWindow::Queue => None,
            DialogWindow::Database => self
                .database
                .as_ref()
                .map(|dialog| database_dialog::body(dialog, tokens)),
            DialogWindow::Properties => self
                .properties
                .as_ref()
                .map(|dialog| properties::body(dialog, tokens)),
            DialogWindow::QuickConnect => self
                .quick
                .as_ref()
                .map(|dialog| quick_connect::body(dialog, tokens)),
            DialogWindow::Network => self
                .drive_bar
                .dialog
                .as_ref()
                .map(|dialog| drive_bar::network_body(dialog, tokens)),
            DialogWindow::Archive => self
                .archive
                .as_ref()
                .filter(|dialog| archive_ops::windowed(dialog))
                .map(|dialog| archive_ops::body(dialog, tokens)),
            DialogWindow::ChecksumForm => self
                .checksum
                .as_ref()
                .filter(|open| !matches!(open, ChecksumWindow::Results(_)))
                .map(|open| checksum::body(open, tokens)),
            DialogWindow::Hotlist => self
                .tabs
                .editor
                .as_ref()
                .map(|editor| tabs::hotlist_body(editor, tokens)),
            DialogWindow::SshExec => self
                .ssh_exec
                .as_ref()
                .map(|dialog| ssh_exec::body(dialog, tokens)),
            DialogWindow::Transfer
            | DialogWindow::NewItem
            | DialogWindow::Delete
            | DialogWindow::Conflict
            | DialogWindow::JobError
            | DialogWindow::Progress => self.file_ops_body(kind, tokens),
            DialogWindow::Question(question) => return self.question_layers_of(question, tokens),
        };
        body.map(|body| layout::window_body(body, tokens))
            .into_iter()
            .collect()
    }

    /// The title of window `id`: the dialog's in its window, the app's in the main window.
    pub(in crate::app) fn window_title(&self, id: iced::window::Id) -> String {
        let Some(kind) = self.dialog_windows.kind_of(id) else {
            return settings::window_title();
        };
        let title = match kind {
            DialogWindow::Settings => t!("settings.title"),
            DialogWindow::Queue => return self.queue_title(),
            DialogWindow::Connections => self.connections.as_ref().map_or("", connections::title),
            DialogWindow::Search => t!("search.title"),
            DialogWindow::MultiRename => t!("multi_rename.title"),
            DialogWindow::Compare => self.compare_title(),
            DialogWindow::Checksum => t!("checksum.result.title"),
            DialogWindow::LoginScripts => t!("login_scripts.title"),
            DialogWindow::Lister(lister) => return self.lister_title(lister),
            DialogWindow::Database => self.database.as_ref().map_or("", database_dialog::title),
            DialogWindow::Properties => self.properties.as_ref().map_or("", properties::title),
            DialogWindow::QuickConnect => t!("quick.title"),
            DialogWindow::Network => self
                .drive_bar
                .dialog
                .as_ref()
                .map_or("", drive_bar::network_title),
            DialogWindow::Archive => self.archive.as_ref().map_or("", archive_ops::title),
            DialogWindow::ChecksumForm => self.checksum.as_ref().map_or("", checksum::title),
            DialogWindow::Hotlist => t!("hotlist.dialog_title"),
            DialogWindow::SshExec => {
                return self
                    .ssh_exec
                    .as_ref()
                    .map_or_else(String::new, ssh_exec::title);
            }
            DialogWindow::Transfer
            | DialogWindow::NewItem
            | DialogWindow::Delete
            | DialogWindow::Conflict
            | DialogWindow::JobError
            | DialogWindow::Progress => self.file_ops_title(kind),
            DialogWindow::Question(question) => return self.question_title(question),
        };
        title.to_owned()
    }
}

/// The field the dialog of `kind` gives the keyboard at its opening: the connection manager's
/// filter (owner, 2026-10-18), multi-rename's name mask, and the first field of a standalone
/// dialog that has one (RC-080-23) - the database dialog's path, the quick connection's host,
/// the network path, the archive's name or target, a checksum form's first field, the command
/// to run, the target or name of a copy, a move, a new folder or text file. The window of a
/// file operation's dialog selects in it what the card does (`App::file_ops_focus`).
pub(in crate::app) fn first_field(kind: DialogWindow) -> Option<&'static str> {
    match kind {
        DialogWindow::Connections => Some(connections::FILTER_FIELD),
        DialogWindow::MultiRename => Some(multi_rename::Field::NameMask.id()),
        DialogWindow::Database => Some(database_dialog::PATH_FIELD),
        DialogWindow::QuickConnect => Some(quick_connect::HOST_FIELD),
        DialogWindow::Network => Some(drive_bar::network::REMOTE_FIELD),
        DialogWindow::Archive => Some(archive_ops::TARGET_FIELD),
        DialogWindow::ChecksumForm => Some(checksum::FIRST_FIELD),
        DialogWindow::SshExec => Some(ssh_exec::COMMAND_FIELD),
        DialogWindow::Transfer | DialogWindow::NewItem => Some(file_ops::FIELD_ID),
        DialogWindow::Question(question) => question.first_field(),
        DialogWindow::Settings
        | DialogWindow::Queue
        | DialogWindow::Search
        | DialogWindow::Compare
        | DialogWindow::Checksum
        | DialogWindow::LoginScripts
        | DialogWindow::Lister(_)
        | DialogWindow::Properties
        | DialogWindow::Hotlist
        | DialogWindow::Delete
        | DialogWindow::Conflict
        | DialogWindow::JobError
        | DialogWindow::Progress => None,
    }
}

#[cfg(test)]
#[path = "kinds_tests.rs"]
mod tests;
