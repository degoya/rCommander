//! The manager's right side: the form of one connection, what saving it sends, and the errors
//! that stand at its fields.
//!
//! The form holds text as typed and turns it into the engine's draft or patch only on saving;
//! the engine validates again and its code lands at the field it names. A secret is typed into a
//! field that never shows a stored value: the form knows only whether one is stored
//! ([`SecretFlags`]), and a typed one leaves it as a `SecretParam` in the plan.
//!
//! The same form edits a shared credential (RC-050-12, [`Subject::Credential`]): its name, its
//! user and its secrets. A connection that points at one shows the choice instead of secret
//! fields of its own.

mod plan;

use rc_engine::{
    AuthMethod, Connection, ConnectionId, ConnectionKind, Credential, CredentialId, DefaultView,
    FolderId, SecretFlags, SecretKind, StoredKey,
};
use rc_vault::SecretString;

/// A control of the form, in the order Tab walks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Field {
    Name,
    Kind,
    Host,
    Port,
    User,
    Auth,
    /// The shared credential a connection takes its user and secrets from (RC-050-12).
    Credential,
    Password,
    KeyPath,
    ImportKey,
    Passphrase,
    /// Remove the stored secret of this kind when saving.
    Forget(SecretKind),
    Script,
    View,
    StartDirectory,
    Notes,
    Favourite,
}

impl Field {
    /// The widget id of the field's control: a text field's, or that of the stop of the focus
    /// ring a choice or a switch is (RC-080-16), by which the manager follows the ring.
    pub fn id(self) -> &'static str {
        match self {
            Self::Name => "rcommander.connections.name",
            Self::Host => "rcommander.connections.host",
            Self::Port => "rcommander.connections.port",
            Self::User => "rcommander.connections.user",
            Self::Password => "rcommander.connections.password",
            Self::KeyPath => "rcommander.connections.key_path",
            Self::Passphrase => "rcommander.connections.passphrase",
            Self::StartDirectory => "rcommander.connections.start_directory",
            Self::Notes => "rcommander.connections.notes",
            Self::Kind => "rcommander.connections.kind",
            Self::Auth => "rcommander.connections.auth",
            Self::Credential => "rcommander.connections.credential",
            Self::ImportKey => "rcommander.connections.import_key",
            Self::Forget(SecretKind::Password) => "rcommander.connections.forget_password",
            Self::Forget(SecretKind::Key) => "rcommander.connections.forget_key",
            Self::Forget(SecretKind::Passphrase) => "rcommander.connections.forget_passphrase",
            Self::Script => "rcommander.connections.script",
            Self::View => "rcommander.connections.view",
            Self::Favourite => "rcommander.connections.favourite",
        }
    }

    /// The field an engine error code is about; `None` for one about the whole save.
    pub fn of_code(code: &str) -> Option<Self> {
        Some(match code {
            "name_empty" | "name_taken" => Self::Name,
            "host_empty" | "host_invalid" => Self::Host,
            "port_invalid" => Self::Port,
            "username_empty" => Self::User,
            "auth_unsupported" => Self::Auth,
            "credential_not_found" => Self::Credential,
            "view_unsupported" => Self::View,
            "secret_empty" => Self::Password,
            "key_format_unknown" | "key_file_unreadable" | "key_path_refused" => Self::KeyPath,
            "key_passphrase_needed" | "key_passphrase_wrong" => Self::Passphrase,
            _ => return None,
        })
    }
}

/// An id no widget has: focusing it takes the keys from every text field.
pub const NO_FIELD: &str = "rcommander.connections.none";

/// A text the form edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Name(String),
    Host(String),
    Port(String),
    User(String),
    KeyPath(String),
    StartDirectory(String),
    Notes(String),
}

/// What a form edits.
#[derive(Debug, Clone, Default)]
pub enum Subject {
    #[default]
    Connection,
    /// A shared credential (RC-050-12): as loaded, `None` while it is new.
    Credential(Option<Box<Credential>>),
}

/// The form of one connection or shared credential, new or stored.
#[derive(Debug, Clone)]
pub struct Form {
    pub subject: Subject,
    /// Where a new connection goes.
    pub folder: Option<FolderId>,
    pub name: String,
    pub kind: ConnectionKind,
    pub host: String,
    pub port: String,
    pub user: String,
    pub auth: AuthMethod,
    pub key_path: String,
    /// Take the key file into the vault when saving.
    pub import_key: bool,
    /// Newly typed; never a stored value.
    pub password: SecretString,
    pub passphrase: SecretString,
    /// Typed secrets shown in clear (only while the vault is not locked).
    pub reveal: bool,
    pub forget: Vec<SecretKind>,
    pub script: Option<i64>,
    pub view: DefaultView,
    pub start_directory: String,
    pub notes: String,
    pub favourite: bool,
    /// Which secrets are stored; never their values.
    pub stored: SecretFlags,
    pub stored_key: Option<StoredKey>,
    /// A connection's shared credential, and whether the choice is offered.
    pub shared: Option<CredentialId>,
    pub offer_shared: bool,
    /// The messages at the fields.
    pub errors: Vec<(Field, String)>,
    /// Something was changed since it was loaded or saved.
    pub dirty: bool,
    /// The connection as loaded, for the patch.
    original: Option<Box<Connection>>,
    original_favourite: bool,
    original_shared: Option<CredentialId>,
}

impl Form {
    /// An empty form for a new connection in `folder`.
    pub fn new(folder: Option<FolderId>) -> Self {
        let kind = ConnectionKind::default();
        Self {
            subject: Subject::Connection,
            folder,
            name: String::new(),
            kind,
            host: String::new(),
            port: kind.default_port().to_string(),
            user: String::new(),
            auth: AuthMethod::default(),
            key_path: String::new(),
            import_key: false,
            password: SecretString::from(""),
            passphrase: SecretString::from(""),
            reveal: false,
            forget: Vec::new(),
            script: None,
            view: DefaultView::default(),
            start_directory: String::new(),
            notes: String::new(),
            favourite: folder.is_none(),
            stored: SecretFlags::default(),
            stored_key: None,
            shared: None,
            offer_shared: false,
            errors: Vec::new(),
            dirty: false,
            original: None,
            original_favourite: folder.is_none(),
            original_shared: None,
        }
    }

    /// An empty form for a new shared credential in `folder`.
    pub fn new_credential(folder: Option<FolderId>) -> Self {
        Self {
            subject: Subject::Credential(None),
            favourite: false,
            original_favourite: false,
            ..Self::new(folder)
        }
    }

    /// The form of a stored shared credential.
    pub fn of_credential(credential: &Credential) -> Self {
        Self {
            subject: Subject::Credential(Some(Box::new(credential.clone()))),
            name: credential.name.clone(),
            user: credential.username.clone().unwrap_or_default(),
            stored: credential.secrets,
            stored_key: credential.key.clone(),
            ..Self::new_credential(credential.folder_id)
        }
    }

    /// Whether it edits a shared credential.
    pub fn is_credential(&self) -> bool {
        matches!(self.subject, Subject::Credential(_))
    }

    /// The stored shared credential it edits; `None` for a new one or a connection.
    pub fn credential_id(&self) -> Option<CredentialId> {
        match &self.subject {
            Subject::Credential(Some(credential)) => Some(credential.id),
            _ => None,
        }
    }

    /// Another shared credential for the connection, or its own secrets for `None`.
    pub fn set_shared(&mut self, shared: Option<CredentialId>) {
        self.shared = shared;
        self.touched(Field::Credential);
    }

    /// The form of a stored connection.
    pub fn of(connection: &Connection, favourite: bool) -> Self {
        Self {
            folder: connection.folder_id,
            name: connection.name.clone(),
            kind: connection.kind,
            host: connection.host.clone(),
            port: connection.port.to_string(),
            user: connection.username.clone().unwrap_or_default(),
            auth: connection.auth_method,
            key_path: connection.key_path.clone().unwrap_or_default(),
            script: connection.login_script_id,
            view: connection.default_view,
            start_directory: connection.start_directory.clone().unwrap_or_default(),
            notes: connection.notes.clone().unwrap_or_default(),
            favourite,
            stored: connection.secrets,
            stored_key: connection.key.clone(),
            shared: connection.credential_id,
            original: Some(Box::new(connection.clone())),
            original_favourite: favourite,
            original_shared: connection.credential_id,
            ..Self::new(connection.folder_id)
        }
    }

    /// A text typed into a field; the field it went to.
    pub fn edit(&mut self, edit: Edit) -> Field {
        let (slot, field) = match edit {
            Edit::Name(text) => ((&mut self.name, text), Field::Name),
            Edit::Host(text) => ((&mut self.host, text), Field::Host),
            Edit::Port(text) => ((&mut self.port, text), Field::Port),
            Edit::User(text) => ((&mut self.user, text), Field::User),
            Edit::KeyPath(text) => ((&mut self.key_path, text), Field::KeyPath),
            Edit::StartDirectory(text) => {
                ((&mut self.start_directory, text), Field::StartDirectory)
            }
            Edit::Notes(text) => ((&mut self.notes, text), Field::Notes),
        };
        *slot.0 = slot.1;
        self.touched(field);
        field
    }

    /// A typed secret; the password field for any kind but the passphrase.
    pub fn secret(&mut self, kind: SecretKind, value: SecretString) {
        if kind == SecretKind::Passphrase {
            self.passphrase = value;
            self.touched(Field::Passphrase);
        } else {
            self.password = value;
            self.touched(Field::Password);
        }
    }

    /// Another protocol; a port at the old one's default follows to the new one's.
    pub fn set_kind(&mut self, kind: ConnectionKind) {
        let default = self.kind.default_port().to_string();
        if self.port.trim().is_empty() || self.port.trim() == default {
            self.port = kind.default_port().to_string();
        }
        self.kind = kind;
        self.touched(Field::Kind);
    }

    pub fn set_auth(&mut self, auth: AuthMethod) {
        self.auth = auth;
        self.touched(Field::Auth);
    }

    pub fn set_view(&mut self, view: DefaultView) {
        self.view = view;
        self.touched(Field::View);
    }

    pub fn set_script(&mut self, script: Option<i64>) {
        self.script = script;
        self.touched(Field::Script);
    }

    /// Flips a switch of the form.
    pub fn toggle(&mut self, field: Field) {
        match field {
            Field::ImportKey => self.import_key = !self.import_key,
            Field::Favourite => self.favourite = !self.favourite,
            Field::Forget(kind) => {
                if let Some(at) = self.forget.iter().position(|forget| *forget == kind) {
                    self.forget.remove(at);
                } else {
                    self.forget.push(kind);
                }
            }
            _ => return,
        }
        self.touched(field);
    }

    fn touched(&mut self, field: Field) {
        self.dirty = true;
        self.errors.retain(|(at, _)| *at != field);
    }

    /// The message at `field`.
    pub fn error(&self, field: Field) -> Option<&str> {
        self.errors
            .iter()
            .find(|(at, _)| *at == field)
            .map(|(_, text)| text.as_str())
    }

    /// Whether the password field is shown: for a password, and keyboard-interactive answers;
    /// always for a shared credential, never for a connection that points at one.
    pub fn uses_password(&self) -> bool {
        self.is_credential()
            || (self.shared.is_none()
                && matches!(self.auth, AuthMethod::Password | AuthMethod::Interactive))
    }

    pub fn uses_key(&self) -> bool {
        self.is_credential() || (self.shared.is_none() && self.auth == AuthMethod::Key)
    }

    /// The controls shown, in Tab order.
    pub fn fields(&self) -> Vec<Field> {
        if self.is_credential() {
            let mut fields = vec![
                Field::Name,
                Field::User,
                Field::Password,
                Field::KeyPath,
                Field::ImportKey,
                Field::Passphrase,
            ];
            fields.extend(self.forget_fields());
            return fields;
        }
        let mut fields = vec![
            Field::Name,
            Field::Kind,
            Field::Host,
            Field::Port,
            Field::User,
            Field::Auth,
        ];
        if self.offer_shared || self.shared.is_some() {
            fields.push(Field::Credential);
        }
        if self.uses_password() {
            fields.push(Field::Password);
        }
        if self.uses_key() {
            fields.extend([Field::KeyPath, Field::ImportKey, Field::Passphrase]);
        }
        if self.shared.is_none() {
            fields.extend(self.forget_fields());
        }
        fields.extend([
            Field::Script,
            Field::View,
            Field::StartDirectory,
            Field::Notes,
            Field::Favourite,
        ]);
        fields
    }

    /// A switch to remove each stored secret.
    fn forget_fields(&self) -> impl Iterator<Item = Field> + '_ {
        SecretKind::ALL
            .into_iter()
            .filter(|kind| self.stored.has(*kind))
            .map(Field::Forget)
    }

    /// The stored connection; `None` while it is new.
    pub fn id(&self) -> Option<ConnectionId> {
        self.original.as_ref().map(|connection| connection.id)
    }

    /// `connection` as stored now under a form whose edits are kept: what is sent next is
    /// measured against it, and the flags of its stored secrets are its.
    pub fn rebase(&mut self, connection: &Connection, favourite: bool) {
        self.folder = connection.folder_id;
        self.stored = connection.secrets;
        self.stored_key = connection.key.clone();
        self.forget.retain(|kind| connection.secrets.has(*kind));
        self.original = Some(Box::new(connection.clone()));
        self.original_favourite = favourite;
        self.original_shared = connection.credential_id;
    }

    /// [`Form::rebase`] for a shared credential's form.
    pub fn rebase_credential(&mut self, credential: &Credential) {
        self.folder = credential.folder_id;
        self.stored = credential.secrets;
        self.stored_key = credential.key.clone();
        self.forget.retain(|kind| credential.secrets.has(*kind));
        self.subject = Subject::Credential(Some(Box::new(credential.clone())));
    }

    /// After a save: the typed secrets and the marks to remove are done with.
    pub fn saved(&mut self) {
        self.password = SecretString::from("");
        self.passphrase = SecretString::from("");
        self.import_key = false;
        self.forget.clear();
        self.dirty = false;
    }
}

#[cfg(test)]
#[path = "form_tests.rs"]
mod tests;
