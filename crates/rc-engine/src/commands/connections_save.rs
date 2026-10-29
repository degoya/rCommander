//! `connections.save`: the manager's form saved in one step (RC-080-25) - the fields, the secrets,
//! the login script and the drive-bar mark in one transaction of `rcommander.db`, so a refusal
//! anywhere leaves the connection exactly as it was stored.
//!
//! What can be refused without writing is asked first, in the form's order: the fields; then
//! each secret change (a key only from its file, a blank value, a key file on a share); then the
//! vault, before a key file is opened (`vault_absent`, `vault_locked`); then each key file, opened
//! once with its passphrase. Only then does the one transaction run, sealing every secret for the
//! connection's row inside it - a new connection's id exists only there.
//!
//! Through MCP no secret is changed and no script that sends one is assigned
//! (`Engine::refuse_mcp_secret_steps`, `gui_only`), and the endpoint of a connection that holds a
//! secret stays as for `connections.update` (`endpoint_change_needs_gui`).

use rc_db::connections::{ConnectionSave, SaveError, SaveTarget, SecretWrite};
use rc_vault::{ExposeSecret as _, SecretString};

use super::{
    favourites::{FAVOURITES_KEY, Favourites},
    secrets::{open_key, read_key_file},
};
use crate::{
    Connection, ConnectionId, ConnectionPatch, Engine, EngineError, EngineEvent, Outcome,
    SaveConnectionRequest, SaveFields, SecretChange, SecretKind, StoredKey, TreeChange, TreeItem,
    read_guard,
    secrets::{field, local_key_path},
};

/// A secret change read and checked, ready to be sealed; a shared credential's save
/// (`credentials.rs`) prepares its changes the same way.
pub(super) enum Ready {
    Clear(SecretKind),
    Value(SecretKind, SecretString),
    Key {
        key: SecretString,
        info: StoredKey,
        /// Only when the key needs one.
        passphrase: Option<SecretString>,
    },
}

impl Engine {
    /// `connections.save`: everything or nothing; answers the connection as saved.
    pub(crate) async fn save_connection(
        &self,
        request: &SaveConnectionRequest,
    ) -> Result<Outcome, EngineError> {
        let db = self.db()?;
        let target = match &request.fields {
            SaveFields::Create {
                folder_id,
                connection,
            } => SaveTarget::Create {
                folder: *folder_id,
                draft: connection,
            },
            SaveFields::Update { id, patch } => SaveTarget::Update { id: *id, patch },
        };
        // An MCP call's look at the endpoint runs under the secret writes and refuses early,
        // before the vault and key files; the save judges it again inside its own transaction,
        // against a secret stored by another process (`endpoint_rule`, RC-080-50).
        let mut held = None;
        if let SaveFields::Update { id, patch } = &request.fields
            && read_guard::from_mcp()
        {
            held = Some(self.inner.secret_writes.lock().await);
            self.refuse_endpoint_move(*id, patch).await?;
        }
        db.check_connection_save(target).await?;
        let ready = self.ready_secrets(&request.secrets).await?;
        let save = ConnectionSave {
            target,
            secrets: writes(&ready),
            login_script: request.login_script,
            credential: request.credential,
        };
        let vault = if request.stores_secrets() {
            Some(self.vault_handle().await?)
        } else {
            None
        };
        let seal = |kind: SecretKind, row: &str| -> Result<Vec<u8>, EngineError> {
            let vault = vault.as_ref().ok_or(EngineError::VaultLocked)?;
            let secret = value_of(&ready, kind).ok_or(EngineError::SecretEmpty)?;
            Ok(vault.encrypt_str(&field(kind, row), secret)?)
        };
        let mark = request.favourite.map(|favourite| {
            (
                FAVOURITES_KEY,
                move |stored: Option<Favourites>, saved: &Connection, ids: &[ConnectionId]| {
                    let mut marks = stored.unwrap_or_default();
                    marks.set(saved, favourite);
                    marks.keep_ids(ids);
                    marks
                },
            )
        });
        let connection = {
            // A secret, a shared credential or a login script makes a connection hold a secret.
            let holds = !request.secrets.is_empty()
                || request.credential.is_some()
                || request.login_script.is_some();
            if held.is_none() && holds {
                held = Some(self.inner.secret_writes.lock().await);
            }
            let _held = held;
            db.save_connection_with(&save, super::connections::endpoint_rule(), seal, mark)
                .await
                .map_err(|error| match error {
                    SaveError::Connection(error) => self.secret_write_failed(error.into()),
                    SaveError::LoginScript(error) => error.into(),
                    SaveError::Seal(error) => error,
                })?
        };
        self.announce_saved(request, &ready, connection.id).await;
        Ok(Outcome::Connection { connection })
    }

    /// Each change checked, then the vault, then each key file opened: nothing is written yet.
    pub(super) async fn ready_secrets(
        &self,
        changes: &[SecretChange],
    ) -> Result<Vec<Ready>, EngineError> {
        for change in changes {
            match change {
                SecretChange::Set {
                    kind: SecretKind::Key,
                    ..
                } => return Err(EngineError::KeyImportOnly),
                SecretChange::Set { value, .. }
                    if value.secret().expose_secret().trim().is_empty() =>
                {
                    return Err(EngineError::SecretEmpty);
                }
                // A share is never opened: on Windows that alone sends the person's credentials.
                SecretChange::ImportKey { path, .. } if !local_key_path(path) => {
                    return Err(EngineError::KeyPathRefused { path: path.clone() });
                }
                _ => {}
            }
        }
        let stores = changes
            .iter()
            .any(|change| !matches!(change, SecretChange::Clear { .. }));
        if stores {
            // Absent or locked is answered before the person waits for a key to open.
            self.vault_handle().await?.touch()?;
        }
        let mut ready = Vec::with_capacity(changes.len());
        for change in changes {
            ready.push(match change {
                SecretChange::Clear { kind } => Ready::Clear(*kind),
                SecretChange::Set { kind, value } => Ready::Value(*kind, value.secret().clone()),
                SecretChange::ImportKey { path, passphrase } => {
                    let key = read_key_file(path).await?;
                    let passphrase = passphrase
                        .as_ref()
                        .filter(|passphrase| !passphrase.is_empty())
                        .map(|passphrase| passphrase.secret().clone());
                    let (info, passphrase) = open_key(&key, passphrase).await?;
                    Ready::Key {
                        key,
                        info,
                        passphrase,
                    }
                }
            });
        }
        Ok(ready)
    }

    /// After the commit: the stored secrets remembered for the MCP mask, a changed connection's
    /// open remote session dropped, and the window told.
    async fn announce_saved(
        &self,
        request: &SaveConnectionRequest,
        ready: &[Ready],
        id: ConnectionId,
    ) {
        self.remember_ready(ready);
        let change = match &request.fields {
            SaveFields::Create { .. } => TreeChange::Created,
            SaveFields::Update { patch, .. } => {
                if *patch != ConnectionPatch::default()
                    || !request.secrets.is_empty()
                    || request.credential.is_some()
                {
                    self.forget_remote(id).await;
                }
                TreeChange::Updated
            }
        };
        tracing::debug!(connection = %id, secrets = ready.len(), "connection saved");
        self.announce(TreeItem::Connection(id), change);
        if let Some(Some(script)) = request.login_script {
            self.publish(EngineEvent::LoginScript {
                script,
                change: TreeChange::Updated,
            });
        }
        if request.favourite.is_some() {
            self.publish(EngineEvent::Drives);
        }
        if ready.iter().any(|ready| matches!(ready, Ready::Clear(_))) {
            self.announce_if_vault_unused().await;
        }
    }

    /// The stored secrets of a save remembered for the MCP mask.
    pub(super) fn remember_ready(&self, ready: &[Ready]) {
        for ready in ready {
            match ready {
                Ready::Clear(_) => {}
                Ready::Value(_, secret) => self.remember_secret(secret),
                Ready::Key {
                    key, passphrase, ..
                } => {
                    self.remember_secret(key);
                    if let Some(passphrase) = passphrase {
                        self.remember_secret(passphrase);
                    }
                }
            }
        }
    }
}

/// What the database writes for each change; the values are sealed inside its transaction.
pub(super) fn writes(ready: &[Ready]) -> Vec<SecretWrite<'_>> {
    ready
        .iter()
        .map(|ready| match ready {
            Ready::Clear(kind) => SecretWrite::Clear(*kind),
            Ready::Value(kind, _) => SecretWrite::Seal(*kind),
            Ready::Key {
                info, passphrase, ..
            } => SecretWrite::Key {
                info,
                passphrase: passphrase.is_some(),
            },
        })
        .collect()
}

/// The value sealed for `kind`: the last change that gives one, a key's passphrase included.
pub(super) fn value_of(ready: &[Ready], kind: SecretKind) -> Option<&SecretString> {
    ready.iter().rev().find_map(|ready| match (ready, kind) {
        (Ready::Value(of, secret), _) if *of == kind => Some(secret),
        (Ready::Key { key, .. }, SecretKind::Key) => Some(key),
        (
            Ready::Key {
                passphrase: Some(passphrase),
                ..
            },
            SecretKind::Passphrase,
        ) => Some(passphrase),
        _ => None,
    })
}

#[cfg(test)]
#[path = "connections_save_tests.rs"]
mod tests;
