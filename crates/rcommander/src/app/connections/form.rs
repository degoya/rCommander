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
//! fields of its own. A connection chooses a colour theme for each mode (RC-100-51): Default (the
//! window's) or a theme of that mode.

mod field;
mod jump;
mod plan;
mod themes;
mod totp;

pub use field::{Field, NO_FIELD};
pub use jump::{chain_text, jump_choices};
pub use themes::theme_choices;
pub use totp::{secret_field, totp_hint};

use rc_engine::{
    AuthMethod, Connection, ConnectionId, ConnectionKind, Credential, CredentialId, DefaultView,
    FolderId, SecretFlags, SecretKind, StoredKey,
};
use rc_vault::SecretString;

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
    /// The one-time code seed, newly typed (RC-060-08).
    pub totp_seed: SecretString,
    /// Typed secrets shown in clear (only while the vault is not locked).
    pub reveal: bool,
    pub forget: Vec<SecretKind>,
    pub script: Option<i64>,
    pub view: DefaultView,
    pub start_directory: String,
    pub notes: String,
    /// The theme id of the light and of the dark mode; `None` for the window's (RC-100-51).
    pub theme_light: Option<String>,
    pub theme_dark: Option<String>,
    pub favourite: bool,
    /// The saved SSH connection it connects through (RC-110-03); `None` connects directly.
    pub jump: Option<ConnectionId>,
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
            totp_seed: SecretString::from(""),
            reveal: false,
            forget: Vec::new(),
            script: None,
            view: DefaultView::default(),
            start_directory: String::new(),
            notes: String::new(),
            theme_light: None,
            theme_dark: None,
            favourite: folder.is_none(),
            jump: None,
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
            theme_light: connection.theme_light.clone(),
            theme_dark: connection.theme_dark.clone(),
            favourite,
            jump: connection.jump_id,
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

    /// A typed secret; the password field for any kind but the passphrase and the seed.
    pub fn secret(&mut self, kind: SecretKind, value: SecretString) {
        if kind == SecretKind::TotpSeed {
            self.totp_seed = value;
            self.touched(Field::TotpSeed);
        } else if kind == SecretKind::Passphrase {
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
        // Only an SSH connection goes through a jump host (RC-110-03).
        if kind != ConnectionKind::Ssh {
            self.jump = None;
        }
        self.touched(Field::Kind);
    }

    /// Another jump host; `None` connects directly (RC-110-03).
    pub fn set_jump(&mut self, jump: Option<ConnectionId>) {
        self.jump = jump;
        self.touched(Field::Jump);
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
        let mut fields = vec![Field::Name, Field::Kind, Field::Host, Field::Port];
        if self.kind == ConnectionKind::Ssh {
            fields.push(Field::Jump);
        }
        fields.extend([Field::User, Field::Auth]);
        if self.offer_shared || self.shared.is_some() {
            fields.push(Field::Credential);
        }
        if self.uses_password() {
            fields.push(Field::Password);
        }
        if self.uses_key() {
            fields.extend([Field::KeyPath, Field::ImportKey, Field::Passphrase]);
        }
        fields.extend(self.totp_fields());
        if self.shared.is_none() {
            fields.extend(self.forget_fields());
        }
        fields.extend([
            Field::Script,
            Field::View,
            Field::StartDirectory,
            Field::Notes,
            Field::ThemeLight,
            Field::ThemeDark,
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
        self.totp_seed = SecretString::from("");
        self.import_key = false;
        self.forget.clear();
        self.dirty = false;
    }
}

#[cfg(test)]
#[path = "form_tests.rs"]
mod tests;
