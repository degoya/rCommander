//! `connections.duplicate` (RC-050-15): a copy of a stored connection in its folder, right under
//! it, named `<name> (2)` - or the next number free there.
//!
//! The copy takes every field, the login script, the shared credential it points at and its own
//! secrets: each is decrypted here and sealed again for the copy's row inside the one transaction
//! of `connections.save` (RC-080-25), so the copy is complete or not there at all. A locked vault
//! answers `vault_locked` before anything is written. The drive-bar mark, the session preferences
//! and references to a secret source (RC-100-34) are not copied. The class is `GuiOnly`: copying
//! secrets is the person's decision, never an agent's.

use rc_db::connections::{Connection, ConnectionSave, SaveError, SaveTarget};

use super::connections_save::{Ready, value_of, writes};
use crate::{
    ConnectionDraft, ConnectionId, Engine, EngineError, Outcome, SecretKind, SecretRef, TreeChange,
    TreeItem, secrets::field,
};

/// The most numbers tried for a free name before the copy is refused as `name_taken`.
const MAX_COPY_NUMBER: u32 = 1000;

impl Engine {
    /// `connections.duplicate`: answers the copy as saved.
    pub(crate) async fn duplicate_connection(
        &self,
        id: ConnectionId,
    ) -> Result<Outcome, EngineError> {
        let db = self.db()?;
        let source = db.connection(id).await?;
        let draft = draft_of(&source, self.copy_name(&source).await?);
        let ready = self.own_secrets(&source).await?;
        let vault = if ready.is_empty() {
            None
        } else {
            Some(self.vault_handle().await?)
        };
        let seal = |kind: SecretKind, row: &str| -> Result<Vec<u8>, EngineError> {
            let vault = vault.as_ref().ok_or(EngineError::VaultLocked)?;
            let secret = value_of(&ready, kind).ok_or(EngineError::SecretEmpty)?;
            Ok(vault.encrypt_str(&field(kind, row), secret)?)
        };
        let save = ConnectionSave {
            target: SaveTarget::Create {
                folder: source.folder_id,
                draft: &draft,
            },
            secrets: writes(&ready),
            login_script: Some(source.login_script_id.map(crate::LoginScriptId)),
            credential: Some(source.credential_id),
        };
        let copy = {
            let _writes = if ready.is_empty() {
                None
            } else {
                Some(self.inner.secret_writes.lock().await)
            };
            db.save_connection(&save, seal, NO_SETTING)
                .await
                .map_err(|error| match error {
                    SaveError::Connection(error) => self.secret_write_failed(error.into()),
                    SaveError::LoginScript(error) => error.into(),
                    SaveError::Seal(error) => error,
                })?
        };
        // Right under the original: the same place, and the name sorts after it.
        let copy = db
            .move_connection(copy.id, source.folder_id, Some(source.sort_order))
            .await?;
        self.remember_ready(&ready);
        tracing::debug!(connection = %id, copy = %copy.id, secrets = ready.len(), "connection duplicated");
        self.announce(TreeItem::Connection(copy.id), TreeChange::Created);
        Ok(Outcome::Connection { connection: copy })
    }

    /// `<name> (2)`, or the first number from 2 up whose name the folder does not hold yet (as
    /// the database compares them, without regard to case); the copy of `web (2)` is `web (3)`,
    /// not `web (2) (2)`.
    async fn copy_name(&self, source: &Connection) -> Result<String, EngineError> {
        let db = self.db()?;
        let base = strip_number(&source.name);
        for number in 2..=MAX_COPY_NUMBER {
            let candidate = format!("{base} ({number})");
            let named = db.connections_named(&candidate).await?;
            if !named
                .iter()
                .any(|other| other.folder_id == source.folder_id)
            {
                return Ok(candidate);
            }
        }
        // The save refuses it with `name_taken`.
        Ok(format!("{base} ({MAX_COPY_NUMBER})"))
    }

    /// The connection's own stored secrets, decrypted, as a save takes them; a locked vault is
    /// `vault_locked` and opens no dialog of its own (the manager asks).
    async fn own_secrets(&self, source: &Connection) -> Result<Vec<Ready>, EngineError> {
        let flags = source.secrets;
        let read = |kind| {
            self.vault_secret(
                SecretRef {
                    connection: source.id,
                    kind,
                },
                false,
            )
        };
        let mut ready = Vec::new();
        if flags.has_password
            && let Some(password) = read(SecretKind::Password).await?
        {
            ready.push(Ready::Value(SecretKind::Password, password));
        }
        let passphrase = if flags.has_passphrase {
            read(SecretKind::Passphrase).await?
        } else {
            None
        };
        match (&source.key, flags.has_key) {
            (Some(info), true) => {
                if let Some(key) = read(SecretKind::Key).await? {
                    ready.push(Ready::Key {
                        key,
                        info: info.clone(),
                        passphrase,
                    });
                }
            }
            // The passphrase of the file at `key_path`.
            _ => {
                if let Some(passphrase) = passphrase {
                    ready.push(Ready::Value(SecretKind::Passphrase, passphrase));
                }
            }
        }
        Ok(ready)
    }
}

/// The edit of a setting that follows a save; a copy changes none.
type SettingEdit = fn(Option<()>, &Connection, &[ConnectionId]);

/// No setting changes with a copy.
const NO_SETTING: Option<(&str, SettingEdit)> = None;

/// The copy's fields: the source's, under `name`.
fn draft_of(source: &Connection, name: String) -> ConnectionDraft {
    ConnectionDraft {
        name,
        kind: source.kind,
        host: source.host.clone(),
        port: Some(i64::from(source.port)),
        username: source.username.clone(),
        auth_method: source.auth_method,
        key_path: source.key_path.clone(),
        default_view: source.default_view,
        start_directory: source.start_directory.clone(),
        notes: source.notes.clone(),
        ftp_data_mode: source.ftp_data_mode,
        ftp_encoding: source.ftp_encoding,
    }
}

/// `name` without a trailing ` (<number>)`.
pub(super) fn strip_number(name: &str) -> &str {
    name.strip_suffix(')')
        .and_then(|rest| rest.rsplit_once(" ("))
        .filter(|(_, number)| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
        .map_or(name, |(base, _)| base)
}

#[cfg(test)]
#[path = "connections_duplicate_tests.rs"]
mod tests;
