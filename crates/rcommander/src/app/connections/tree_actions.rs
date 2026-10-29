//! What the tree does: new folders and connections, renaming, deleting after a question, moving
//! by Ctrl+X and Ctrl+V or by dragging, duplicating (Shift+F5, RC-050-15) and the drive-bar
//! mark. Each is one command; the tree comes back through the engine's event.

use rc_engine::{
    Command, ConnectionId, ConnectionPatch, CredentialFields, CredentialPatch, FolderId, Outcome,
    SaveCredentialRequest, TreeItem,
};

use super::{
    ConnectionManager, Focus, NameFor, Pending,
    form::{Field, Form},
    jobs::Then,
    keys::focus_id,
    run,
    tree::contents,
    vault::{AfterVault, Ask, Retry},
    view::NAME_FIELD,
};
use crate::{
    app::Effect,
    i18n::{error_text, t},
};

impl ConnectionManager {
    /// Insert: an empty form for a connection in the folder at the cursor.
    pub(super) fn new_connection(&mut self) -> Vec<Effect> {
        let folder = self.tree.target_folder(self.tree.current());
        self.form = Some(Form::new(folder));
        self.fit_form();
        self.pending = None;
        self.focus = Focus::Field(Field::Name);
        vec![focus_id(Field::Name.id())]
    }

    /// F7 (`parent` `None`: the folder at the cursor): a name for a new folder.
    pub(super) fn ask_name(&mut self, parent: Option<FolderId>) -> Vec<Effect> {
        let parent = parent.or_else(|| self.tree.target_folder(self.tree.current()));
        self.pending = Some(Pending::Name {
            target: NameFor::NewFolder(parent),
            text: t!("connections.new_folder_name").to_owned(),
        });
        vec![focus_id(NAME_FIELD), select_all(NAME_FIELD)]
    }

    /// Shift+F5 or the button: a copy of the connection at the cursor, secrets included.
    pub(super) fn duplicate(&mut self) -> Vec<Effect> {
        let Some(TreeItem::Connection(id)) = self.tree.current() else {
            self.note = Some((t!("connections.duplicate_none").to_owned(), true));
            return Vec::new();
        };
        self.pending = None;
        vec![run(
            Then::Duplicated(id),
            Command::DuplicateConnection { id },
        )]
    }

    /// The copy of `id`: the cursor on it and its name open to change, as F2 does; a locked
    /// vault asks for the master password and duplicates again.
    pub(super) fn duplicated(
        &mut self,
        id: ConnectionId,
        result: Result<Outcome, &'static str>,
    ) -> Vec<Effect> {
        match result {
            Ok(Outcome::Connection { connection }) => {
                let item = TreeItem::Connection(connection.id);
                self.tree.cursor = Some(item);
                self.show(Some(item));
                self.focus = Focus::Tree;
                self.pending = Some(Pending::Name {
                    target: NameFor::Rename(item),
                    text: connection.name,
                });
                vec![focus_id(NAME_FIELD), select_all(NAME_FIELD)]
            }
            Ok(_) => Vec::new(),
            Err(code) => match Ask::for_code(code) {
                Some(ask) => {
                    self.vault = Some(ask.state());
                    let again = Command::DuplicateConnection { id };
                    let retry = Retry::Manager(Then::Duplicated(id), Box::new(again));
                    self.ask_vault(ask, AfterVault::Retry(retry))
                }
                None => {
                    self.note = Some((error_text(code), true));
                    Vec::new()
                }
            },
        }
    }

    /// F2: the name of the folder or connection at the cursor, to change.
    pub(super) fn ask_rename(&mut self) -> Vec<Effect> {
        let Some(item) = self.tree.current() else {
            return Vec::new();
        };
        self.pending = Some(Pending::Name {
            target: NameFor::Rename(item),
            text: self.tree.name_of(item),
        });
        vec![focus_id(NAME_FIELD), select_all(NAME_FIELD)]
    }

    /// Enter in the name field: the folder is made, or the item renamed.
    pub(super) fn submit_name(&mut self) -> Vec<Effect> {
        let Some(Pending::Name { target, text }) = self.pending.take() else {
            return Vec::new();
        };
        self.focus = Focus::Tree;
        let name = text.trim().to_owned();
        if name.is_empty() {
            self.note = Some((error_text("name_empty"), true));
            return Vec::new();
        }
        let command = match target {
            NameFor::NewFolder(parent_id) => {
                if let Some(parent) = parent_id {
                    self.tree.fold(parent, Some(true));
                }
                Command::CreateFolder { parent_id, name }
            }
            NameFor::Rename(TreeItem::Folder(id)) => Command::RenameFolder { id, name },
            NameFor::Rename(TreeItem::Connection(id)) => Command::UpdateConnection {
                id,
                patch: ConnectionPatch {
                    name: Some(name),
                    ..ConnectionPatch::default()
                },
            },
            NameFor::Rename(TreeItem::Credential(id)) => {
                Command::SaveCredential(SaveCredentialRequest {
                    fields: CredentialFields::Update {
                        id,
                        patch: CredentialPatch {
                            name: Some(name),
                            ..CredentialPatch::default()
                        },
                    },
                    secrets: Vec::new(),
                })
            }
        };
        vec![run(Then::Select, command), unfocus()]
    }

    /// F8 or Delete: the question, with what a folder holds or how many connections use a
    /// shared credential.
    pub(super) fn ask_delete(&mut self) -> Vec<Effect> {
        let Some(item) = self.tree.current() else {
            return Vec::new();
        };
        let (folders, connections) = match item {
            TreeItem::Folder(id) => self.tree.folder(id).map_or((1, 0), contents),
            TreeItem::Connection(_) => (0, 1),
            TreeItem::Credential(id) => (0, self.tree.credential(id).map_or(0, |c| c.connections)),
        };
        self.pending = Some(Pending::Delete {
            item,
            folders,
            connections,
        });
        Vec::new()
    }

    /// Yes to the question: the item goes, a folder with everything inside it; the cursor moves
    /// to its neighbour.
    pub(super) fn confirm_delete(&mut self) -> Vec<Effect> {
        let Some(Pending::Delete { item, .. }) = self.pending.take() else {
            return Vec::new();
        };
        self.tree.cursor = self.tree.neighbour(item);
        if self.clipboard == Some(item) {
            self.clipboard = None;
        }
        let command = match item {
            TreeItem::Folder(id) => Command::DeleteFolder {
                id,
                recursive: true,
            },
            TreeItem::Connection(id) => Command::DeleteConnection { id },
            TreeItem::Credential(id) => Command::DeleteCredential { id },
        };
        self.show(self.tree.cursor);
        vec![run(Then::Changed, command)]
    }

    /// Ctrl+X: the item at the cursor waits for Ctrl+V.
    pub(super) fn cut(&mut self) {
        self.clipboard = self.tree.current();
        if self.clipboard.is_some() {
            self.note = Some((t!("connections.cut").to_owned(), false));
        }
    }

    /// Ctrl+V: the cut item moves into the folder at the cursor.
    pub(super) fn paste(&mut self) -> Vec<Effect> {
        let Some(item) = self.clipboard.take() else {
            return Vec::new();
        };
        let target = self.tree.target_folder(self.tree.current());
        self.note = None;
        self.move_into(item, target)
    }

    /// `item` into `folder` (the root for `None`), after what is there.
    fn move_into(&mut self, item: TreeItem, folder: Option<FolderId>) -> Vec<Effect> {
        if self.tree.parent_of(item) == folder {
            return Vec::new();
        }
        if let Some(folder) = folder {
            self.tree.fold(folder, Some(true));
        }
        self.tree.cursor = Some(item);
        let command = match item {
            TreeItem::Folder(id) => Command::MoveFolder(rc_engine::MoveFolderRequest {
                id,
                parent_id: folder,
                sort_order: None,
            }),
            TreeItem::Connection(id) => Command::MoveConnection(rc_engine::MoveConnectionRequest {
                id,
                folder_id: folder,
                sort_order: None,
            }),
            TreeItem::Credential(id) => Command::MoveCredential {
                id,
                folder_id: folder,
            },
        };
        vec![run(Then::Select, command)]
    }

    /// A row pressed: the cursor goes there and a drag may start.
    pub(super) fn press(&mut self, item: TreeItem) -> Vec<Effect> {
        self.drag = Some(item);
        self.focus = Focus::Tree;
        self.pending = None;
        if self.tree.cursor != Some(item) {
            self.tree.cursor = Some(item);
            self.show(Some(item));
        }
        vec![unfocus()]
    }

    /// Released over a row: a drag from another row drops there.
    pub(super) fn release(&mut self, target: TreeItem) -> Vec<Effect> {
        let Some(item) = self.drag.take() else {
            return Vec::new();
        };
        if item == target {
            return Vec::new();
        }
        let folder = self.tree.target_folder(Some(target));
        self.move_into(item, folder)
    }

    /// Enter or a double click: a folder opens or closes, an SSH server opens in the active
    /// panel (`remote.rs`), a shared credential's name takes the keys.
    pub(super) fn activate(&mut self, item: TreeItem) -> Vec<Effect> {
        match item {
            TreeItem::Folder(id) => self.tree.fold(id, None),
            TreeItem::Connection(id) => return self.open_connection(id),
            TreeItem::Credential(_) => {
                self.focus = Focus::Field(Field::Name);
                return vec![focus_id(Field::Name.id())];
            }
        }
        Vec::new()
    }

    /// Space: the connection at the cursor is listed in the drive bar, or no longer.
    pub(super) fn toggle_favourite(&mut self) -> Vec<Effect> {
        let Some(TreeItem::Connection(id)) = self.tree.current() else {
            return Vec::new();
        };
        let favourite = !self.is_favourite(id);
        vec![run(Then::Drives, Command::SetFavourite { id, favourite })]
    }
}

fn select_all(id: &'static str) -> Effect {
    Effect::Task(iced::widget::operation::select_all(iced::widget::Id::new(
        id,
    )))
}

/// Takes the keys from every text field: the tree has them.
pub(super) fn unfocus() -> Effect {
    focus_id(super::form::NO_FIELD)
}
