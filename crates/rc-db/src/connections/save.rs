//! Saving a connection as the manager's form does, in one transaction (RC-080-25): its fields,
//! its secrets, its login script and a setting that follows from it (the drive bar's marks) -
//! all of it, or nothing.
//!
//! The caller seals the secrets: the database moves bytes, never a plaintext. `seal` is called
//! inside the transaction with the connection's row id, so a new connection's secrets are sealed
//! for the id it has just been given; when it fails, nothing is written.

use serde::{Serialize, de::DeserializeOwned};
use sqlx::Connection as _;

use crate::{
    Db, DbError, SecretKind, SecretOwner, StoredKey,
    connections::{
        Connection, ConnectionDraft, ConnectionError, ConnectionId, ConnectionPatch, CredentialId,
        EndpointRule, FolderId, TreeResult,
        credentials::write_credential_link,
        endpoint::guard_endpoint,
        fetch,
        folders::require_folder,
        store::{apply_patch, insert, require_free_name},
        validate::Fields,
    },
    login_scripts::{LoginScriptError, LoginScriptId, write_login_script},
    secrets::{write_key, write_secret},
    settings::{read_setting, write_setting},
};

/// What a save does to the connection's own fields.
#[derive(Debug, Clone, Copy)]
pub enum SaveTarget<'a> {
    /// A new connection in `folder` (the root when `None`).
    Create {
        folder: Option<FolderId>,
        draft: &'a ConnectionDraft,
    },
    /// The changes to a stored one; an empty patch changes no field.
    Update {
        id: ConnectionId,
        patch: &'a ConnectionPatch,
    },
}

/// One change of a stored secret; a secret not named stays as it is.
#[derive(Debug, Clone, Copy)]
pub enum SecretWrite<'a> {
    /// Removed.
    Clear(SecretKind),
    /// Stored as `seal` makes it for the row.
    Seal(SecretKind),
    /// A private key taken into the vault: its ciphertext from `seal` for [`SecretKind::Key`],
    /// its public half `info` and, with `passphrase`, the passphrase's ciphertext from `seal`
    /// for [`SecretKind::Passphrase`]; without one the stored passphrase stays.
    Key {
        info: &'a StoredKey,
        passphrase: bool,
    },
}

impl SecretWrite<'_> {
    /// Whether this change stores a value (and so needs the vault key this process holds).
    fn stores(&self) -> bool {
        !matches!(self, Self::Clear(_))
    }
}

/// Everything one save writes, applied in this order: the fields, the secrets in the order
/// given, the login script, the shared credential.
#[derive(Debug, Clone)]
pub struct ConnectionSave<'a> {
    pub target: SaveTarget<'a>,
    pub secrets: Vec<SecretWrite<'a>>,
    /// The login script, when it changes; `Some(None)` runs none.
    pub login_script: Option<Option<LoginScriptId>>,
    /// The shared credential it takes its user and secrets from, when that changes; `Some(None)`
    /// uses its own again (RC-050-12).
    pub credential: Option<Option<CredentialId>>,
}

/// What a save refuses or fails at; nothing of it is written.
#[derive(Debug)]
pub enum SaveError<E> {
    /// The tree refused a field, or the database failed.
    Connection(ConnectionError),
    /// The login script is not there.
    LoginScript(LoginScriptError),
    /// `seal` failed.
    Seal(E),
}

impl<E> From<ConnectionError> for SaveError<E> {
    fn from(error: ConnectionError) -> Self {
        Self::Connection(error)
    }
}

impl<E> From<DbError> for SaveError<E> {
    fn from(error: DbError) -> Self {
        Self::Connection(ConnectionError::Db(error))
    }
}

impl<E> From<sqlx::Error> for SaveError<E> {
    fn from(error: sqlx::Error) -> Self {
        Self::Connection(error.into())
    }
}

impl Db {
    /// Checks the fields of a save as [`Db::save_connection`] will, without writing, so that a
    /// field's refusal comes before anything else is asked (the save checks again).
    ///
    /// # Errors
    ///
    /// The refusal of a field, the folder or the connection missing, or the read fails.
    pub async fn check_connection_save(&self, target: SaveTarget<'_>) -> TreeResult<()> {
        let mut reader = self.readers().acquire().await?;
        match target {
            SaveTarget::Create { folder, draft } => {
                let fields = Fields::from_draft(draft)?;
                if let Some(folder) = folder {
                    require_folder(&mut reader, folder).await?;
                }
                require_free_name(&mut reader, folder, &fields.name, None).await
            }
            SaveTarget::Update { id, patch } => {
                let current = fetch(&mut reader, id).await?;
                if *patch == ConnectionPatch::default() {
                    return Ok(());
                }
                let fields = Fields::patched(&current, patch)?;
                require_free_name(&mut reader, current.folder_id, &fields.name, Some(id)).await
            }
        }
    }

    /// Saves a connection in one `BEGIN IMMEDIATE` transaction: its fields, then its secrets
    /// (each sealed by `seal` for its kind and the connection's row id), then its login script
    /// and its shared credential;
    /// then, with `setting`, the setting `key` rewritten by `edit` from its stored value (`None`
    /// when unset or unreadable), the saved connection and the ids of every connection. Answers
    /// the connection as saved.
    ///
    /// # Errors
    ///
    /// [`SaveError::Connection`] for a refused field, a missing folder or connection,
    /// [`DbError::VaultChanged`] (inside it) when a secret is stored and the vault key this
    /// process holds is no longer the vault's, or a failed write; [`SaveError::LoginScript`]
    /// for a missing script; [`ConnectionError::CredentialNotFound`] inside
    /// [`SaveError::Connection`] for a missing credential; [`SaveError::Seal`] when `seal` fails.
    /// In every case nothing is written.
    pub async fn save_connection<E, T, F>(
        &self,
        save: &ConnectionSave<'_>,
        seal: impl FnMut(SecretKind, &str) -> Result<Vec<u8>, E>,
        setting: Option<(&str, F)>,
    ) -> Result<Connection, SaveError<E>>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce(Option<T>, &Connection, &[ConnectionId]) -> T,
    {
        self.save_connection_with(save, EndpointRule::Free, seal, setting)
            .await
    }

    /// [`Db::save_connection`] under `rule`, judged inside the save's transaction before
    /// anything is written: with [`EndpointRule::KeepWhileSecretsHeld`] an update that changes
    /// the host, port, kind or user of a connection holding a secret is refused (RC-080-50).
    ///
    /// # Errors
    ///
    /// As [`Db::save_connection`], and [`ConnectionError::EndpointChangeNeedsGui`] inside
    /// [`SaveError::Connection`].
    pub async fn save_connection_with<E, T, F>(
        &self,
        save: &ConnectionSave<'_>,
        rule: EndpointRule,
        mut seal: impl FnMut(SecretKind, &str) -> Result<Vec<u8>, E>,
        setting: Option<(&str, F)>,
    ) -> Result<Connection, SaveError<E>>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce(Option<T>, &Connection, &[ConnectionId]) -> T,
    {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        if let SaveTarget::Update { id, patch } = save.target
            && rule != EndpointRule::Free
        {
            let current = fetch(&mut tx, id).await?;
            guard_endpoint(&mut tx, &current, patch, rule).await?;
        }
        let id = match save.target {
            SaveTarget::Create { folder, draft } => {
                insert(&mut tx, folder, &Fields::from_draft(draft)?).await?
            }
            SaveTarget::Update { id, patch } if *patch == ConnectionPatch::default() => {
                fetch(&mut tx, id).await?.id
            }
            SaveTarget::Update { id, patch } => apply_patch(&mut tx, id, patch).await?.id,
        };
        if save.secrets.iter().any(SecretWrite::stores) {
            self.check_vault_key(&mut tx).await?;
        }
        let row = id.0.to_string();
        let owner = SecretOwner::Connection(id);
        for write in &save.secrets {
            match *write {
                SecretWrite::Clear(kind) => write_secret(&mut tx, owner, kind, None).await?,
                SecretWrite::Seal(kind) => {
                    let cipher = seal(kind, &row).map_err(SaveError::Seal)?;
                    write_secret(&mut tx, owner, kind, Some(&cipher)).await?;
                }
                SecretWrite::Key { info, passphrase } => {
                    let key = seal(SecretKind::Key, &row).map_err(SaveError::Seal)?;
                    let passphrase = passphrase
                        .then(|| seal(SecretKind::Passphrase, &row))
                        .transpose()
                        .map_err(SaveError::Seal)?;
                    write_key(&mut tx, owner, &key, info, passphrase.as_deref()).await?;
                }
            }
        }
        if let Some(script) = save.login_script {
            write_login_script(&mut tx, id, script)
                .await
                .map_err(SaveError::LoginScript)?;
        }
        if let Some(credential) = save.credential {
            write_credential_link(&mut tx, id, credential).await?;
        }
        let connection = fetch(&mut tx, id).await?;
        if let Some((key, edit)) = setting {
            let stored = read_setting(&mut tx, key).await?.and_then(|text| {
                serde_json::from_str(&text)
                    .inspect_err(|error| tracing::warn!(key, %error, "a setting does not read"))
                    .ok()
            });
            let ids: Vec<ConnectionId> =
                sqlx::query_scalar("SELECT id FROM connections ORDER BY id")
                    .fetch_all(&mut *tx)
                    .await?
                    .into_iter()
                    .map(ConnectionId)
                    .collect();
            let value = edit(stored, &connection, &ids);
            let text = serde_json::to_string(&value).map_err(|source| DbError::Json {
                key: key.to_owned(),
                source,
            })?;
            write_setting(&mut tx, key, &text).await?;
        }
        tx.commit().await?;
        Ok(connection)
    }
}

#[cfg(test)]
#[path = "save_tests.rs"]
mod tests;
