//! The connection manager (RC-050-04): a dialog with the connection tree on the left and the
//! form of one connection on the right, after a small Remote Desktop Manager for SSH. Ctrl+F
//! opens it, as in Total Commander (`connections.manager`), and so does the status bar.
//!
//! The manager keeps no data of its own: the tree, the drive bar's marks, the vault's state and
//! the login scripts are the engine's, read through commands (`jobs.rs`) and read again when
//! the engine announces a change - so a connection an agent creates over MCP appears at once.
//! The keyboard does everything (`keys.rs`): arrows, Enter, F2 rename, F7 new folder, F8 or
//! Delete delete, Insert new connection, Ctrl+X and Ctrl+V move, Space marks for the drive bar,
//! Tab into the form; the mouse adds clicks and dragging in the tree (`tree_actions.rs`).
//! Secrets are typed into fields that never show a stored value; the master password is asked
//! only when one is about to be stored (`vault.rs`). The ssh_config import of RC-050-06 is a
//! page of the dialog (`import.rs`), as is the import from Remote Desktop Manager (`rdm_import.rs`,
//! RC-050-11); so is the Total Commander import, which the settings open (`tc_import.rs`,
//! RC-050-10). Shared credentials (RC-050-12) are rows of the tree with a form of their own
//! (`credentials.rs`), offered while the setting `vault.shared_credentials` is on.

mod credentials;
mod follow;
mod form;
mod import;
mod jobs;
mod keys;
mod preview;
mod rdm_import;
mod remote;
mod save;
mod session;
mod sources;
mod start;
mod tc_import;
mod tree;
mod tree_actions;
mod vault;
mod view;

use rc_engine::{
    AuthMethod, Command, ConnectionId, ConnectionKind, DefaultView, DriveGroup, EngineEvent,
    Outcome, SHARED_CREDENTIALS_KEY, SecretKind, TreeItem, VaultStateKind,
};
use rc_vault::SecretString;

pub use self::jobs::ConnJob;
pub use self::keys::Focus;
#[cfg(test)]
pub(super) use self::view::TREE_STOP_ID;
pub(super) use self::{
    form::Edit,
    import::ImportMsg,
    rdm_import::RdmImportMsg,
    sources::SourceMsg,
    tc_import::TcImportMsg,
    view::{FILTER_FIELD, VAULT_FIELD, body, layer, prompt_body, prompt_title, title},
};
use self::{
    form::{Field, Form},
    import::ImportDialog,
    jobs::Then,
    rdm_import::RdmImportDialog,
    sources::SourcePanel,
    tc_import::TcImportDialog,
    tree::TreeState,
    vault::VaultPrompt,
};
use super::{App, Effect, Message};
use crate::i18n::error_text;

/// A name being typed into the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameFor {
    /// A new folder in this folder (the root for `None`).
    NewFolder(Option<rc_engine::FolderId>),
    Rename(TreeItem),
}

/// A question of the tree that waits for Enter or Escape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pending {
    Name {
        target: NameFor,
        text: String,
    },
    /// Delete this, with what it holds.
    Delete {
        item: TreeItem,
        folders: u64,
        connections: u64,
    },
}

/// The open manager.
#[derive(Debug, Default)]
pub struct ConnectionManager {
    pub tree: TreeState,
    /// The connection shown on the right; `None` before one is chosen.
    pub form: Option<Form>,
    pub focus: Focus,
    pub pending: Option<Pending>,
    /// Cut with Ctrl+X, moved with Ctrl+V.
    pub clipboard: Option<TreeItem>,
    /// Pressed in the tree and not yet released.
    pub drag: Option<TreeItem>,
    /// The connections the drive bar lists.
    pub favourites: Vec<ConnectionId>,
    /// The trusted FTPS certificates, shown under an FTPS connection's form (RC-070-02).
    pub certificates: Vec<rc_db::TrustedCertificate>,
    /// The secret sources and the shown connection's references (RC-100-34).
    pub sources: SourcePanel,
    /// The login scripts by id and name.
    pub scripts: Vec<(i64, String)>,
    /// The SSH sessions up or being set up (`Engine::ssh_sessions`).
    pub sessions: Vec<rc_engine::SessionStatus>,
    pub vault: Option<VaultStateKind>,
    pub prompt: Option<VaultPrompt>,
    /// The last outcome: a text, and whether it is an error.
    pub note: Option<(String, bool)>,
    /// A save runs.
    pub busy: bool,
    /// The import page, while it is shown.
    pub import: Option<ImportDialog>,
    /// The page of the import from Total Commander, while it is shown.
    pub tc_import: Option<TcImportDialog>,
    /// The page of the import from Remote Desktop Manager (RC-050-11), while it is shown.
    pub rdm_import: Option<RdmImportDialog>,
    /// Opened by the settings for that import alone: leaving its page closes the manager.
    pub import_only: bool,
    /// A key or Escape asked to close the dialog.
    pub close: bool,
    /// The row the cursor was on when the manager closed last in this run (RC-050-15).
    pub start: Option<TreeItem>,
    /// The first tree has put the cursor on its starting row.
    pub started: bool,
    /// Opened only for the master password a connection needs: the prompt alone, closed with
    /// it.
    pub prompt_only: bool,
}

/// What the dialog's controls and the engine's answers say.
#[derive(Debug, Clone)]
pub enum ConnMsg {
    /// Ctrl+F or the status bar.
    Open,
    Close,
    /// The answer to a command.
    Answer(Then, Box<Result<Outcome, &'static str>>),
    Saved(Result<ConnectionId, &'static str>),
    /// Keys taken into the vault after an import, by host alias.
    KeysTaken(Vec<(String, Result<(), &'static str>)>),
    Filter(String),
    /// A row was pressed, released or double-clicked.
    Press(TreeItem),
    Release(TreeItem),
    Activate(TreeItem),
    NameInput(String),
    NameSubmit,
    /// The answer to the delete question.
    Confirm(bool),
    NewConnection,
    NewFolder,
    /// A copy of the connection at the cursor, with its secrets (RC-050-15).
    Duplicate,
    /// A new shared credential in the folder at the cursor (RC-050-12).
    NewCredential,
    /// The shared credential a connection points at; its own secrets for `None`.
    Shared(Option<rc_engine::CredentialId>),
    Edit(Edit),
    Secret(SecretKind, SecretString),
    Kind(ConnectionKind),
    Auth(AuthMethod),
    View(DefaultView),
    Script(Option<i64>),
    Toggle(Field),
    Reveal(bool),
    Save,
    /// The master-password prompt: a field (`true` for the repetition), Enter, Escape.
    VaultInput(bool, SecretString),
    VaultSubmit,
    VaultCancel,
    Import(ImportMsg),
    /// The page of the import from Total Commander (RC-050-09).
    TcImport(TcImportMsg),
    /// The page of the import from Remote Desktop Manager (RC-050-11).
    RdmImport(RdmImportMsg),
    /// Test, open or close a stored connection (`connections.test`, `.connect`, `.disconnect`).
    Test(ConnectionId),
    Connect(ConnectionId),
    Disconnect(ConnectionId),
    /// The login-script editor, over the manager (RC-050-08).
    LoginScripts,
    /// An SSH connection in the active panel, a shell on it, one command on it (RC-060-02,
    /// RC-060-03, RC-060-04).
    Files(ConnectionId),
    Terminal(ConnectionId),
    RunCommand(ConnectionId),
    /// Forget a trusted FTPS certificate, by its id (RC-070-02).
    ForgetCertificate(i64),
    /// The secret-source panel under the form (RC-100-34).
    Source(SourceMsg),
    /// After Tab: the id of the stop of the focus ring that has the keyboard (RC-080-16).
    Focused(Option<iced::widget::Id>),
}

/// Whether the manager takes `event`; the drive bar may take it as well.
pub fn concerns(event: &EngineEvent) -> bool {
    matches!(
        event,
        EngineEvent::ConnectionManager
            | EngineEvent::ConnectionTree { .. }
            | EngineEvent::Drives
            | EngineEvent::VaultLocked
            | EngineEvent::VaultUnlocked
            | EngineEvent::VaultRemoved
            | EngineEvent::LoginScript { .. }
            | EngineEvent::SecretNeeded { .. }
            | EngineEvent::Session { .. }
            | EngineEvent::TrustedCertificatesChanged
    ) || matches!(event, EngineEvent::SettingChanged { key } if key == SHARED_CREDENTIALS_KEY)
}

/// A command for the engine whose answer comes back as `then`.
fn run(then: Then, command: Command) -> Effect {
    Effect::Connections(ConnJob::Run(then, Box::new(command)))
}

impl App {
    /// A message of the dialog.
    pub(super) fn on_connections(&mut self, message: ConnMsg) -> Vec<Effect> {
        match message {
            ConnMsg::Open => self.open_connections(),
            ConnMsg::LoginScripts => self.open_login_scripts(),
            ConnMsg::Files(_) | ConnMsg::Terminal(_) | ConnMsg::RunCommand(_) => {
                self.connection_action(message)
            }
            ConnMsg::Close => {
                self.connections = None;
                Vec::new()
            }
            ConnMsg::Import(ImportMsg::Open) => {
                let wsl = import::wsl_roots(&self.drive_bar.drives);
                let Some(manager) = self.connections.as_mut() else {
                    return Vec::new();
                };
                manager.open_import(wsl)
            }
            ConnMsg::TcImport(TcImportMsg::Open) => self.open_tc_import(),
            message => {
                let Some(manager) = self.connections.as_mut() else {
                    return Vec::new();
                };
                let mut effects = manager.message(message);
                effects.extend(manager.sources_follow());
                self.close_connections_if_asked();
                effects
            }
        }
    }

    /// While the dialog is open, keys and clicks belong to it; `None` for a message it does
    /// not take.
    pub(super) fn connections_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        let manager = self.connections.as_mut()?;
        let effects = match message {
            Message::Key(chord) => manager.key(chord, false),
            Message::KeyCaptured(chord) => manager.key(chord, true),
            Message::Press => Vec::new(),
            _ => return None,
        };
        let mut effects = effects;
        effects.extend(manager.sources_follow());
        self.close_connections_if_asked();
        Some(effects)
    }

    /// An event of [`concerns`].
    pub(super) fn on_connections_event(&mut self, event: &EngineEvent) -> Vec<Effect> {
        if *event == EngineEvent::ConnectionManager {
            return self.open_connections();
        }
        if let EngineEvent::SecretNeeded { .. } = event {
            return self.secret_needed();
        }
        if let EngineEvent::Session { .. } = event {
            self.sessions_changed();
            return Vec::new();
        }
        let Some(manager) = self.connections.as_mut() else {
            return Vec::new();
        };
        match event {
            // The drive bar's servers follow the tree: a new root connection is one.
            EngineEvent::ConnectionTree { .. } => vec![
                run(Then::Tree, Command::ListConnections),
                run(Then::Drives, Command::ListDrives { refresh: false }),
            ],
            EngineEvent::Drives => vec![run(Then::Drives, Command::ListDrives { refresh: false })],
            EngineEvent::TrustedCertificatesChanged => vec![remote::list_certificates()],
            EngineEvent::SettingChanged { .. } => vec![credentials::read_setting()],
            EngineEvent::LoginScript { .. } => vec![run(Then::Scripts, Command::ListLoginScripts)],
            EngineEvent::VaultLocked => {
                manager.vault = Some(VaultStateKind::Locked);
                if let Some(form) = &mut manager.form {
                    form.reveal = false;
                }
                Vec::new()
            }
            EngineEvent::VaultUnlocked => {
                manager.vault = Some(VaultStateKind::Unlocked);
                Vec::new()
            }
            EngineEvent::VaultRemoved => {
                manager.vault = Some(VaultStateKind::Absent);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn open_connections(&mut self) -> Vec<Effect> {
        // In a window of its own, Ctrl+F brings it to the front (RC-080-15).
        if self.connections.is_some() {
            return self.dialog_window_front(super::dialog_windows::DialogWindow::Connections);
        }
        // The keyboard in the list, on the connection used last; typing there filters at once
        // (owner, 2026-10-26; `start.rs`).
        self.connections = Some(ConnectionManager {
            sessions: self.engine.ssh_sessions(),
            focus: Focus::Tree,
            start: self.connections_cursor,
            ..ConnectionManager::default()
        });
        vec![
            keys::focus_id(view::TREE_STOP_ID),
            start::read_focus_setting(),
            run(Then::Tree, Command::ListConnections),
            run(Then::Drives, Command::ListDrives { refresh: false }),
            run(Then::Vault, Command::GetVaultStatus),
            run(Then::Scripts, Command::ListLoginScripts),
            remote::list_certificates(),
            sources::list_sources(),
            credentials::read_setting(),
        ]
    }

    fn close_connections_if_asked(&mut self) {
        if let Some(manager) = &self.connections {
            self.connections_cursor = manager.tree.cursor;
        }
        if self
            .connections
            .as_ref()
            .is_some_and(|manager| manager.close)
        {
            self.connections = None;
        }
    }
}

impl ConnectionManager {
    fn message(&mut self, message: ConnMsg) -> Vec<Effect> {
        match message {
            ConnMsg::Open | ConnMsg::Close | ConnMsg::LoginScripts => Vec::new(),
            ConnMsg::Test(_) | ConnMsg::Connect(_) | ConnMsg::Disconnect(_) => {
                self.session_message(message)
            }
            ConnMsg::Answer(then, result) => self.answer(then, *result),
            ConnMsg::Focused(id) => self.ring_moved(id.as_ref()),
            ConnMsg::Saved(result) => self.saved(result),
            ConnMsg::KeysTaken(taken) => self.keys_taken(taken),
            ConnMsg::Filter(text) => {
                self.tree.filter = text;
                self.focus = Focus::Filter;
                Vec::new()
            }
            ConnMsg::Press(item) => self.press(item),
            ConnMsg::Release(item) => self.release(item),
            ConnMsg::Activate(item) => self.activate(item),
            ConnMsg::NameInput(text) => {
                if let Some(Pending::Name { text: typed, .. }) = &mut self.pending {
                    *typed = text;
                }
                Vec::new()
            }
            ConnMsg::NameSubmit => self.submit_name(),
            ConnMsg::Confirm(true) => self.confirm_delete(),
            ConnMsg::Confirm(false) => {
                self.pending = None;
                Vec::new()
            }
            ConnMsg::NewConnection => self.new_connection(),
            ConnMsg::NewFolder => self.ask_name(None),
            ConnMsg::Duplicate => self.duplicate(),
            ConnMsg::NewCredential => self.new_credential(),
            ConnMsg::ForgetCertificate(id) => self.forget_certificate(id),
            ConnMsg::Source(message) => self.on_source(message),
            ConnMsg::Import(message) => self.on_import(message),
            ConnMsg::TcImport(message) => self.on_tc_import(message),
            ConnMsg::RdmImport(message) => self.on_rdm_import(message),
            message => self.form_message(message),
        }
    }

    /// The engine answered a command.
    fn answer(&mut self, then: Then, result: Result<Outcome, &'static str>) -> Vec<Effect> {
        match (then, result) {
            (Then::Tree, Ok(Outcome::ConnectionTree { tree })) => {
                self.tree.tree = tree;
                self.refresh_form();
                return self.place_start();
            }
            (Then::FocusSetting, result) => return self.focus_setting(result),
            (Then::Duplicated(id), result) => return self.duplicated(id, result),
            (Then::Drives, Ok(Outcome::Drives { drives })) => {
                self.favourites = drives
                    .iter()
                    .filter(|drive| drive.group == DriveGroup::Servers && drive.favourite)
                    .filter_map(|drive| drive.id.strip_prefix("server:")?.parse().ok())
                    .map(ConnectionId)
                    .collect();
                self.refresh_form();
            }
            (Then::Vault, Ok(Outcome::Vault(status))) => self.vault = Some(status.state),
            (Then::Scripts, Ok(Outcome::LoginScripts { scripts })) => {
                self.scripts = scripts
                    .into_iter()
                    .map(|script| (script.id.0, script.name))
                    .collect();
            }
            // A list of scripts that cannot be read leaves only "none" to choose.
            (Then::Scripts, Err(_)) => self.scripts.clear(),
            (Then::Select, Ok(Outcome::Connection { connection })) => {
                self.tree.cursor = Some(TreeItem::Connection(connection.id));
                self.refresh_form();
            }
            (Then::Select, Ok(Outcome::Folder { folder })) => {
                self.tree.cursor = Some(TreeItem::Folder(folder.id));
                self.refresh_form();
            }
            (Then::Select | Then::CredentialSaved | Then::SharedSetting, result) => {
                return self.credential_answer(then, result);
            }
            (Then::VaultOpened, result) => return self.vault_opened(result),
            (Then::Preview(_) | Then::Imported, result) => return self.import_answer(then, result),
            (Then::Connect(id) | Then::Tested(id), Err("vault_locked")) => {
                return self.session_refused(then, id);
            }
            (Then::Tested(_), result) => self.tested(result),
            (Then::Certificates, Ok(Outcome::TrustedCertificates { certificates })) => {
                self.certificates = certificates;
            }
            (Then::TcPreview(_) | Then::TcImported | Then::TcPasswords, result) => {
                return self.tc_import_answer(then, result);
            }
            (Then::RdmPreview(_) | Then::RdmImported, result) => {
                return self.rdm_import_answer(then, result);
            }
            (Then::Sources | Then::SourceFound(_) | Then::SourceMapped, result) => {
                return self.source_answer(then, result);
            }
            (_, Err(code)) => self.note = Some((error_text(code), true)),
            // Every other answer is announced as an event too.
            (_, Ok(_)) => {}
        }
        Vec::new()
    }

    /// Whether the drive bar lists `id`.
    fn is_favourite(&self, id: ConnectionId) -> bool {
        self.favourites.contains(&id)
    }
}

#[cfg(test)]
#[path = "connections_tests.rs"]
mod tests;
