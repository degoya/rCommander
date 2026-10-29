//! Shared credentials (`credentials`, migration 0018, RC-050-12): a user and the secrets that go
//! with it, kept once in a folder of the tree, and the connections that point at one.
//!
//! An entry is written as the manager's form saves it, in one transaction: its name and user,
//! then its secrets, each sealed by the caller for the entry's row ([`Db::save_credential`]). As
//! for a connection, no type here holds a secret: an entry says which are stored
//! ([`SecretFlags`]) and shows a stored key by its public half, never a value.

use serde::{Deserialize, Serialize};
use sqlx::{Connection as _, Row as _, SqliteConnection, sqlite::SqliteRow};

use crate::{
    Db, SecretFlags, SecretKind, SecretOwner, StoredKey,
    connections::{
        ConnectionError, ConnectionId, CredentialId, FolderId, SaveError, SecretWrite, TreeResult,
        folders::require_folder, validate, write_error,
    },
    secrets::{write_key, write_secret},
};

/// `SELECT <every credential column> FROM credentials <tail>`, as one static string.
macro_rules! select_credentials {
    ($tail:literal) => {
        concat!(
            "SELECT id, folder_id, name, username, created_at, updated_at, \
             password_enc IS NOT NULL AS has_password, key_enc IS NOT NULL AS has_key, \
             passphrase_enc IS NOT NULL AS has_passphrase, key_type, key_fingerprint, \
             key_comment, key_public, \
             (SELECT count(*) FROM connections WHERE credential_id = credentials.id) AS users \
             FROM credentials ",
            $tail
        )
    };
}

/// One shared credential of the tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credential {
    pub id: CredentialId,
    /// The folder it sits in; `None` at the root.
    pub folder_id: Option<FolderId>,
    pub name: String,
    /// The user of every connection that points at it and names none of its own.
    pub username: Option<String>,
    /// RFC 3339 in UTC, as SQLite writes it.
    pub created_at: String,
    pub updated_at: String,
    /// Which secrets are stored (`has_password`, `has_key`, `has_passphrase`); never a value.
    #[serde(flatten)]
    pub secrets: SecretFlags,
    /// The private key stored in the vault, by its public half.
    pub key: Option<StoredKey>,
    /// How many connections point at it.
    pub connections: u64,
}

/// A new shared credential. A user that is empty or blank is stored as none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialDraft {
    pub name: String,
    #[serde(default)]
    pub username: Option<String>,
}

/// A change to a shared credential: `None` keeps a field; an empty user clears it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

/// What a save does to the entry's own fields.
#[derive(Debug, Clone, Copy)]
pub enum CredentialTarget<'a> {
    /// A new entry in `folder` (the root when `None`).
    Create {
        folder: Option<FolderId>,
        draft: &'a CredentialDraft,
    },
    /// The changes to a stored one; an empty patch changes no field.
    Update {
        id: CredentialId,
        patch: &'a CredentialPatch,
    },
}

/// Everything one save of an entry writes: its fields, then its secrets in the order given.
#[derive(Debug, Clone)]
pub struct CredentialSave<'a> {
    pub target: CredentialTarget<'a>,
    pub secrets: Vec<SecretWrite<'a>>,
}

impl Db {
    /// Every shared credential, by name (ignoring case).
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn credentials(&self) -> TreeResult<Vec<Credential>> {
        let mut reader = self.readers().acquire().await?;
        all(&mut reader).await
    }

    /// The shared credential `id`.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::CredentialNotFound`], or the read fails.
    pub async fn credential(&self, id: CredentialId) -> TreeResult<Credential> {
        let mut reader = self.readers().acquire().await?;
        fetch(&mut reader, id).await
    }

    /// The shared credential connection `id` points at; `None` when it uses its own secrets.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn credential_of(&self, id: ConnectionId) -> TreeResult<Option<Credential>> {
        let row = sqlx::query(select_credentials!(
            "WHERE id = (SELECT credential_id FROM connections WHERE id = ?)"
        ))
        .bind(id.0)
        .fetch_optional(self.readers())
        .await?;
        Ok(row.as_ref().map(credential_from_row).transpose()?)
    }

    /// The connections that point at the shared credential `id`, by id.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn credential_users(&self, id: CredentialId) -> TreeResult<Vec<ConnectionId>> {
        let ids: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM connections WHERE credential_id = ? ORDER BY id")
                .bind(id.0)
                .fetch_all(self.readers())
                .await?;
        Ok(ids.into_iter().map(ConnectionId).collect())
    }

    /// Checks the fields of a save as [`Db::save_credential`] will, without writing, so that a
    /// field's refusal comes before the vault is asked (the save checks again).
    ///
    /// # Errors
    ///
    /// The refusal of a field, the folder or the entry missing, or the read fails.
    pub async fn check_credential_save(&self, target: CredentialTarget<'_>) -> TreeResult<()> {
        let mut reader = self.readers().acquire().await?;
        match target {
            CredentialTarget::Create { folder, draft } => {
                let (name, _) = fields(draft)?;
                if let Some(folder) = folder {
                    require_folder(&mut reader, folder).await?;
                }
                require_free_name(&mut reader, folder, &name, None).await
            }
            CredentialTarget::Update { id, patch } => {
                let current = fetch(&mut reader, id).await?;
                let (name, _) = patched(&current, patch)?;
                require_free_name(&mut reader, current.folder_id, &name, Some(id)).await
            }
        }
    }

    /// Saves a shared credential in one `BEGIN IMMEDIATE` transaction: its fields, then its
    /// secrets, each sealed by `seal` for its kind and the entry's row id. Answers the entry as
    /// saved.
    ///
    /// # Errors
    ///
    /// [`SaveError::Connection`] for a refused field, a missing folder or entry,
    /// [`crate::DbError::VaultChanged`] (inside it) when a secret is stored and the vault key
    /// this process holds is no longer the vault's, or a failed write; [`SaveError::Seal`] when
    /// `seal` fails. In every case nothing is written.
    pub async fn save_credential<E>(
        &self,
        save: &CredentialSave<'_>,
        mut seal: impl FnMut(SecretKind, &str) -> Result<Vec<u8>, E>,
    ) -> Result<Credential, SaveError<E>> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let id = match save.target {
            CredentialTarget::Create { folder, draft } => insert(&mut tx, folder, draft).await?,
            CredentialTarget::Update { id, patch } => update(&mut tx, id, patch).await?,
        };
        if save
            .secrets
            .iter()
            .any(|write| !matches!(write, SecretWrite::Clear(_)))
        {
            self.check_vault_key(&mut tx).await?;
        }
        let row = id.0.to_string();
        let owner = SecretOwner::Credential(id);
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
        let credential = fetch(&mut tx, id).await?;
        tx.commit().await?;
        Ok(credential)
    }

    /// Moves a shared credential into `folder` (the root when `None`).
    ///
    /// # Errors
    ///
    /// The entry or folder missing, its name taken in the folder, or the write fails.
    pub async fn move_credential(
        &self,
        id: CredentialId,
        folder: Option<FolderId>,
    ) -> TreeResult<Credential> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let current = fetch(&mut tx, id).await?;
        if let Some(folder) = folder {
            require_folder(&mut tx, folder).await?;
        }
        require_free_name(&mut tx, folder, &current.name, Some(id)).await?;
        sqlx::query(
            "UPDATE credentials SET folder_id = ?, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(folder.map(|folder| folder.0))
        .bind(id.0)
        .execute(&mut *tx)
        .await
        .map_err(|error| write_error(error, &current.name))?;
        let credential = fetch(&mut tx, id).await?;
        tx.commit().await?;
        Ok(credential)
    }

    /// Deletes a shared credential with its secrets; the connections that pointed at it use
    /// their own again. Answers how many did.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::CredentialNotFound`], or the write fails.
    pub async fn delete_credential(&self, id: CredentialId) -> TreeResult<u64> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let users = fetch(&mut tx, id).await?.connections;
        // The foreign key clears `connections.credential_id`.
        sqlx::query("DELETE FROM credentials WHERE id = ?")
            .bind(id.0)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(users)
    }
}

/// Every shared credential by name, read on `connection` (the tree's snapshot).
pub(crate) async fn all(connection: &mut SqliteConnection) -> TreeResult<Vec<Credential>> {
    let rows = sqlx::query(select_credentials!("ORDER BY name COLLATE NOCASE, id"))
        .fetch_all(&mut *connection)
        .await?;
    Ok(rows
        .iter()
        .map(credential_from_row)
        .collect::<sqlx::Result<Vec<_>>>()?)
}

/// Points connection `id` at `credential`, or at none, on the writer inside its transaction.
pub(crate) async fn write_credential_link(
    connection: &mut SqliteConnection,
    id: ConnectionId,
    credential: Option<CredentialId>,
) -> TreeResult<()> {
    if let Some(credential) = credential {
        fetch(connection, credential).await?;
    }
    let done = sqlx::query(
        "UPDATE connections SET credential_id = ?, \
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
    )
    .bind(credential.map(|credential| credential.0))
    .bind(id.0)
    .execute(&mut *connection)
    .await?;
    if done.rows_affected() == 0 {
        return Err(ConnectionError::ConnectionNotFound { id });
    }
    Ok(())
}

/// The name and user of a new entry, checked.
fn fields(draft: &CredentialDraft) -> TreeResult<(String, Option<String>)> {
    Ok((
        validate::name(&draft.name)?,
        validate::user(draft.username.as_deref())?,
    ))
}

/// The name and user of `current` with `patch` applied, checked.
fn patched(current: &Credential, patch: &CredentialPatch) -> TreeResult<(String, Option<String>)> {
    let name = validate::name(patch.name.as_deref().unwrap_or(&current.name))?;
    let username = match &patch.username {
        Some(user) => validate::user(Some(user))?,
        None => current.username.clone(),
    };
    Ok((name, username))
}

async fn insert(
    connection: &mut SqliteConnection,
    folder: Option<FolderId>,
    draft: &CredentialDraft,
) -> TreeResult<CredentialId> {
    let (name, username) = fields(draft)?;
    if let Some(folder) = folder {
        require_folder(connection, folder).await?;
    }
    require_free_name(connection, folder, &name, None).await?;
    let done = sqlx::query("INSERT INTO credentials (folder_id, name, username) VALUES (?, ?, ?)")
        .bind(folder.map(|folder| folder.0))
        .bind(&name)
        .bind(&username)
        .execute(&mut *connection)
        .await
        .map_err(|error| write_error(error, &name))?;
    Ok(CredentialId(done.last_insert_rowid()))
}

async fn update(
    connection: &mut SqliteConnection,
    id: CredentialId,
    patch: &CredentialPatch,
) -> TreeResult<CredentialId> {
    let current = fetch(connection, id).await?;
    if *patch == CredentialPatch::default() {
        return Ok(id);
    }
    let (name, username) = patched(&current, patch)?;
    require_free_name(connection, current.folder_id, &name, Some(id)).await?;
    sqlx::query(
        "UPDATE credentials SET name = ?, username = ?, \
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
    )
    .bind(&name)
    .bind(&username)
    .bind(id.0)
    .execute(&mut *connection)
    .await
    .map_err(|error| write_error(error, &name))?;
    Ok(id)
}

/// The entry `id`, read on `connection`.
async fn fetch(connection: &mut SqliteConnection, id: CredentialId) -> TreeResult<Credential> {
    let row = sqlx::query(select_credentials!("WHERE id = ?"))
        .bind(id.0)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or(ConnectionError::CredentialNotFound { id })?;
    Ok(credential_from_row(&row)?)
}

/// Fails with [`ConnectionError::NameTaken`] when another entry of `folder` (other than
/// `except`) has `name`, ignoring ASCII case like the unique index.
async fn require_free_name(
    connection: &mut SqliteConnection,
    folder: Option<FolderId>,
    name: &str,
    except: Option<CredentialId>,
) -> TreeResult<()> {
    let taken: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM credentials WHERE ifnull(folder_id, 0) = ifnull(?, 0) \
         AND name = ? COLLATE NOCASE AND id <> ?)",
    )
    .bind(folder.map(|folder| folder.0))
    .bind(name)
    .bind(except.map_or(0, |id| id.0))
    .fetch_one(&mut *connection)
    .await?;
    if taken {
        return Err(ConnectionError::NameTaken {
            name: name.to_owned(),
        });
    }
    Ok(())
}

fn credential_from_row(row: &SqliteRow) -> sqlx::Result<Credential> {
    Ok(Credential {
        id: CredentialId(row.try_get("id")?),
        folder_id: row.try_get::<Option<i64>, _>("folder_id")?.map(FolderId),
        name: row.try_get("name")?,
        username: row.try_get("username")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        secrets: SecretFlags::from_row(row)?,
        key: StoredKey::from_row(row)?,
        connections: u64::try_from(row.try_get::<i64, _>("users")?).unwrap_or(0),
    })
}

#[cfg(test)]
#[path = "credentials_tests.rs"]
mod tests;
