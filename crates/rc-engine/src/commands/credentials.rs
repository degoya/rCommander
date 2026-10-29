//! The `credentials.*` commands (RC-050-12): shared credentials in the connection tree - a user
//! and the secrets that go with it, kept once and pointed at by any number of connections.
//!
//! A connection that points at one takes its secrets from it, and its user when it names none of
//! its own ([`Engine::shared_credential`], [`Engine::login_connection`]); with the setting
//! `vault.shared_credentials` off, every connection uses its own again. An entry is saved as a
//! connection is (`connections_save.rs`): its fields and its secrets in one transaction, each
//! secret sealed for the entry's row (`"credentials/<column>/<id>"`).
//!
//! An agent may list, add, rename, move and delete entries, but never store a secret in one,
//! change the user its connections log in as, or point a connection at one
//! (`Engine::refuse_mcp_secret_steps`): any of those would hand a stored secret to whoever the
//! agent chose.

use rc_db::{
    CREDENTIALS_TABLE,
    connections::{CredentialSave, CredentialTarget, SaveError},
};

use super::connections_save::{Ready, value_of, writes};
use crate::{
    Connection, ConnectionId, Credential, CredentialFields, CredentialId, Engine, EngineError,
    FolderId, Outcome, SaveCredentialRequest, TreeChange, TreeItem, secrets::field_in,
};

/// The setting that lets connections take their user and secrets from a shared credential.
pub const SHARED_CREDENTIALS_KEY: &str = "vault.shared_credentials";

impl Engine {
    /// The shared credential connection `id` takes its secrets from; `None` when it points at
    /// none or the setting is off.
    pub(crate) async fn shared_credential(
        &self,
        id: ConnectionId,
    ) -> Result<Option<Credential>, EngineError> {
        if !self.setting_on(SHARED_CREDENTIALS_KEY).await {
            return Ok(None);
        }
        Ok(self.db()?.credential_of(id).await?)
    }

    /// Connection `id` as it logs in: without a user of its own, its shared credential's; it is
    /// remembered as the connection used last (RC-050-15).
    pub(crate) async fn login_connection(
        &self,
        id: ConnectionId,
    ) -> Result<Connection, EngineError> {
        let mut connection = self.db()?.connection(id).await?;
        self.remember_used(id).await;
        if connection.username.is_none()
            && let Some(credential) = self.shared_credential(id).await?
        {
            connection.username = credential.username;
        }
        Ok(connection)
    }

    /// `credentials.save`: everything or nothing; answers the entry as saved.
    pub(crate) async fn save_credential(
        &self,
        request: &SaveCredentialRequest,
    ) -> Result<Outcome, EngineError> {
        let db = self.db()?;
        let target = match &request.fields {
            CredentialFields::Create {
                folder_id,
                credential,
            } => CredentialTarget::Create {
                folder: *folder_id,
                draft: credential,
            },
            CredentialFields::Update { id, patch } => CredentialTarget::Update { id: *id, patch },
        };
        db.check_credential_save(target).await?;
        let ready = self.ready_secrets(&request.secrets).await?;
        let save = CredentialSave {
            target,
            secrets: writes(&ready),
        };
        let stores = ready.iter().any(|ready| !matches!(ready, Ready::Clear(_)));
        let vault = if stores {
            Some(self.vault_handle().await?)
        } else {
            None
        };
        let seal = |kind, row: &str| -> Result<Vec<u8>, EngineError> {
            let vault = vault.as_ref().ok_or(EngineError::VaultLocked)?;
            let secret = value_of(&ready, kind).ok_or(EngineError::SecretEmpty)?;
            Ok(vault.encrypt_str(&field_in(CREDENTIALS_TABLE, kind, row), secret)?)
        };
        let credential = {
            let _writes = if request.secrets.is_empty() {
                None
            } else {
                Some(self.inner.secret_writes.lock().await)
            };
            db.save_credential(&save, seal)
                .await
                .map_err(|error| match error {
                    SaveError::Connection(error) => self.secret_write_failed(error.into()),
                    SaveError::LoginScript(error) => error.into(),
                    SaveError::Seal(error) => error,
                })?
        };
        self.remember_ready(&ready);
        let change = match request.fields {
            CredentialFields::Create { .. } => TreeChange::Created,
            CredentialFields::Update { .. } => {
                // Its connections log in with what changed from their next session on.
                self.forget_users(credential.id).await;
                TreeChange::Updated
            }
        };
        tracing::debug!(credential = %credential.id, secrets = ready.len(), "credential saved");
        self.announce(TreeItem::Credential(credential.id), change);
        if ready.iter().any(|ready| matches!(ready, Ready::Clear(_))) {
            self.announce_if_vault_unused().await;
        }
        Ok(Outcome::Credential { credential })
    }

    /// `credentials.move`.
    pub(crate) async fn move_credential(
        &self,
        id: CredentialId,
        folder_id: Option<FolderId>,
    ) -> Result<Outcome, EngineError> {
        let credential = self.db()?.move_credential(id, folder_id).await?;
        self.announce(TreeItem::Credential(id), TreeChange::Moved);
        Ok(Outcome::Credential { credential })
    }

    /// `credentials.delete`: its secrets go with it; the connections that used it fall back to
    /// their own, their open remote sessions closed.
    pub(crate) async fn delete_credential(&self, id: CredentialId) -> Result<Outcome, EngineError> {
        let users = self.db()?.credential_users(id).await?;
        self.db()?.delete_credential(id).await?;
        for user in users {
            self.forget_remote(user).await;
        }
        let item = TreeItem::Credential(id);
        self.announce(item, TreeChange::Deleted);
        self.announce_if_vault_unused().await;
        Ok(Outcome::Deleted {
            item,
            folders: 0,
            connections: 0,
        })
    }

    /// Drops the open remote sessions of every connection that points at credential `id`; a
    /// failing look is only logged, the change it follows has happened.
    async fn forget_users(&self, id: CredentialId) {
        let users = match self.db() {
            Ok(db) => db.credential_users(id).await.map_err(EngineError::from),
            Err(error) => Err(error),
        };
        match users {
            Ok(users) => {
                for user in users {
                    self.forget_remote(user).await;
                }
            }
            Err(error) => tracing::warn!(%error, "cannot tell which connections use a credential"),
        }
    }
}

/// Whether an agent's save of a shared credential is refused: it stores or removes a secret, or
/// changes the user every connection on it logs in as.
pub(crate) fn credential_needs_gui(request: &SaveCredentialRequest) -> bool {
    let changes_user = matches!(
        &request.fields,
        CredentialFields::Update { patch, .. } if patch.username.is_some()
    );
    changes_user || !request.secrets.is_empty()
}

#[cfg(test)]
#[path = "credentials_tests.rs"]
mod tests;
