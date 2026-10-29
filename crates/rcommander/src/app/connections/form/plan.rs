//! What saving a form sends: one `connections.save` with the connection's fields as a draft or
//! a patch, its secret changes, its login script, its shared credential and its drive-bar mark -
//! each only when it changed. The engine stores all of it in one transaction, or nothing
//! (RC-080-25). A shared credential's form sends one `credentials.save` (RC-050-12).

use rc_engine::{
    Connection, ConnectionDraft, ConnectionPatch, CredentialDraft, CredentialFields,
    CredentialPatch, LoginScriptId, SaveConnectionRequest, SaveCredentialRequest, SaveFields,
    SecretChange, SecretKind, SecretParam,
};
use rc_vault::{ExposeSecret as _, SecretString};

use super::{Field, Form, Subject};

impl Form {
    /// What saving sends, or the fields that cannot be saved as typed. The engine checks the
    /// same and more; this spares the round trip for what the form can tell alone.
    pub fn plan(&self) -> Result<SaveConnectionRequest, Vec<(Field, &'static str)>> {
        let mut invalid = Vec::new();
        if self.name.trim().is_empty() {
            invalid.push((Field::Name, "name_empty"));
        }
        let host = self.host.trim();
        if host.is_empty() {
            invalid.push((Field::Host, "host_empty"));
        } else if host.chars().any(|c| c.is_whitespace() || c.is_control()) {
            invalid.push((Field::Host, "host_invalid"));
        }
        let port = match self.port.trim() {
            "" => None,
            text => match text.parse::<u16>() {
                Ok(port) if port != 0 => Some(i64::from(port)),
                _ => {
                    invalid.push((Field::Port, "port_invalid"));
                    None
                }
            },
        };
        if !invalid.is_empty() {
            return Err(invalid);
        }
        let fields = match &self.original {
            Some(original) => SaveFields::Update {
                id: original.id,
                patch: self.patch(original, port),
            },
            None => SaveFields::Create {
                folder_id: self.folder,
                connection: self.draft(port),
            },
        };
        let original_script = self.original.as_ref().and_then(|c| c.login_script_id);
        Ok(SaveConnectionRequest {
            fields,
            secrets: self.secret_changes(),
            login_script: (self.script != original_script)
                .then_some(self.script.map(LoginScriptId)),
            favourite: (self.favourite != self.original_favourite).then_some(self.favourite),
            credential: (self.shared != self.original_shared).then_some(self.shared),
        })
    }

    /// What saving a shared credential's form sends, or the name that cannot be saved.
    pub fn credential_plan(&self) -> Result<SaveCredentialRequest, Vec<(Field, &'static str)>> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(vec![(Field::Name, "name_empty")]);
        }
        let user = self.user.trim();
        let fields = match &self.subject {
            Subject::Credential(Some(original)) => CredentialFields::Update {
                id: original.id,
                patch: CredentialPatch {
                    name: (name != original.name).then(|| name.to_owned()),
                    username: (user != original.username.as_deref().unwrap_or_default())
                        .then(|| user.to_owned()),
                },
            },
            _ => CredentialFields::Create {
                folder_id: self.folder,
                credential: CredentialDraft {
                    name: name.to_owned(),
                    username: optional(user),
                },
            },
        };
        Ok(SaveCredentialRequest {
            fields,
            secrets: self.secret_changes(),
        })
    }

    /// The field the engine's refusal `code` of a save stands at: a field's code at its
    /// field, a blank secret at the one typed blank, a key's at the key file - but a wrong
    /// passphrase at the passphrase - and a missing script at the script.
    pub fn field_of(&self, code: &str) -> Option<Field> {
        match code {
            "secret_empty" => {
                let blank = |secret: &SecretString| {
                    let value = secret.expose_secret();
                    !value.is_empty() && value.trim().is_empty()
                };
                if self.uses_password() && blank(&self.password) {
                    Some(Field::Password)
                } else {
                    Some(Field::Passphrase)
                }
            }
            "login_script_not_found" => Some(Field::Script),
            _ => Field::of_code(code),
        }
    }

    fn draft(&self, port: Option<i64>) -> ConnectionDraft {
        let mut draft = ConnectionDraft::new(self.name.trim(), self.host.trim());
        draft.kind = self.kind;
        draft.port = port;
        draft.username = optional(&self.user);
        draft.auth_method = self.auth;
        draft.key_path = optional(&self.key_path);
        draft.default_view = self.view;
        draft.start_directory = optional(&self.start_directory);
        draft.notes = optional(&self.notes);
        draft
    }

    /// The fields that differ from `original`; the empty patch when none does. An emptied
    /// optional text is sent as `""`, which clears it.
    fn patch(&self, original: &Connection, port: Option<i64>) -> ConnectionPatch {
        let text = |typed: &str, stored: &Option<String>| {
            let typed = typed.trim();
            (typed != stored.as_deref().unwrap_or_default()).then(|| typed.to_owned())
        };
        let port = port.unwrap_or_else(|| i64::from(self.kind.default_port()));
        ConnectionPatch {
            name: (self.name.trim() != original.name).then(|| self.name.trim().to_owned()),
            kind: (self.kind != original.kind).then_some(self.kind),
            host: (self.host.trim() != original.host).then(|| self.host.trim().to_owned()),
            port: (port != i64::from(original.port)).then_some(port),
            username: text(&self.user, &original.username),
            auth_method: (self.auth != original.auth_method).then_some(self.auth),
            key_path: text(&self.key_path, &original.key_path),
            default_view: (self.view != original.default_view).then_some(self.view),
            start_directory: text(&self.start_directory, &original.start_directory),
            notes: text(&self.notes, &original.notes),
            ..ConnectionPatch::default()
        }
    }

    /// Removals first, then what was typed; a key is taken in only from its file. A connection
    /// on a shared credential changes none of its own.
    fn secret_changes(&self) -> Vec<SecretChange> {
        if !self.is_credential() && self.shared.is_some() {
            return Vec::new();
        }
        let mut changes: Vec<SecretChange> = self
            .forget
            .iter()
            .filter(|kind| self.stored.has(**kind))
            .map(|kind| SecretChange::Clear { kind: *kind })
            .collect();
        let typed = |secret: &SecretString| {
            let value = secret.expose_secret();
            (!value.is_empty()).then(|| SecretParam::new(value.to_owned()))
        };
        if self.uses_password()
            && let Some(password) = typed(&self.password)
        {
            changes.push(SecretChange::Set {
                kind: SecretKind::Password,
                value: password,
            });
        }
        if self.uses_key() {
            let passphrase = typed(&self.passphrase);
            if self.import_key && !self.key_path.trim().is_empty() {
                changes.push(SecretChange::ImportKey {
                    path: self.key_path.trim().to_owned(),
                    passphrase,
                });
            } else if let Some(passphrase) = passphrase {
                changes.push(SecretChange::Set {
                    kind: SecretKind::Passphrase,
                    value: passphrase,
                });
            }
        }
        changes
    }
}

fn optional(text: &str) -> Option<String> {
    Some(text.trim())
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}
