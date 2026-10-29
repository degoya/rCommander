//! The secrets of a connection as the database holds them: rc-vault ciphertexts in the `*_enc`
//! columns of `connections` (migration 0006), never a plaintext.
//!
//! The database moves bytes; `rc-vault` encrypts and decrypts them with
//! `"connections/<column>/<id>"` as associated data, the same triple [`crate::SECRET_COLUMNS`]
//! names for the password change. A connection as the tree reads it carries only
//! [`SecretFlags`] (which secrets are stored) and, for a stored private key, its public half
//! ([`StoredKey`]): never a value.

use serde::{Deserialize, Serialize};
use sqlx::{AssertSqlSafe, Connection as _, Row as _, SqliteConnection, sqlite::SqliteRow};

use crate::{
    Db,
    connections::{Connection, ConnectionError, ConnectionId, CredentialId, TreeResult, fetch},
};

/// The table that holds the secrets; the first part of their associated data.
pub const SECRETS_TABLE: &str = "connections";

/// The table that holds the shared credentials' secrets (RC-050-12).
pub const CREDENTIALS_TABLE: &str = "credentials";

/// The column whose value is a secret's row id in its associated data.
pub const SECRETS_ROW_ID: &str = "id";

/// What a stored secret is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretKind {
    /// The password the connection logs in with.
    Password,
    /// A private key's content (OpenSSH or PEM), in place of a key file.
    Key,
    /// The passphrase of the stored key or of the file at `key_path`.
    Passphrase,
}

impl SecretKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 3] = [Self::Password, Self::Key, Self::Passphrase];

    /// The column that holds this kind's ciphertext.
    #[must_use]
    pub fn column(self) -> &'static str {
        match self {
            Self::Password => "password_enc",
            Self::Key => "key_enc",
            Self::Passphrase => "passphrase_enc",
        }
    }

    /// The name serde and the i18n keys use.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Key => "key",
            Self::Passphrase => "passphrase",
        }
    }

    /// The statement that reads this kind's ciphertext from `table`'s row.
    fn select(self, table: &str) -> String {
        format!("SELECT {} FROM {table} WHERE id = ?", self.column())
    }

    /// The statement that writes this kind's ciphertext into `table`'s row.
    fn update(self, table: &str) -> String {
        let column = self.column();
        // The key's public half goes with it; a key is stored by `write_key`.
        let public = if self == Self::Key {
            ", key_type = NULL, key_fingerprint = NULL, key_comment = NULL, key_public = NULL"
        } else {
            ""
        };
        format!(
            "UPDATE {table} SET {column} = ?{public}, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?"
        )
    }
}

/// Whose secrets: a connection's own, or those of a shared credential that connections point at
/// (RC-050-12). Both rows have the same secret columns; the table is the first part of a
/// secret's associated data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretOwner {
    Connection(ConnectionId),
    Credential(CredentialId),
}

impl SecretOwner {
    /// The table that holds the owner's secrets.
    #[must_use]
    pub fn table(self) -> &'static str {
        match self {
            Self::Connection(_) => SECRETS_TABLE,
            Self::Credential(_) => CREDENTIALS_TABLE,
        }
    }

    /// The owner's row id, as its secrets' associated data names it.
    #[must_use]
    pub fn row(self) -> i64 {
        match self {
            Self::Connection(id) => id.0,
            Self::Credential(id) => id.0,
        }
    }

    fn missing(self) -> ConnectionError {
        match self {
            Self::Connection(id) => ConnectionError::ConnectionNotFound { id },
            Self::Credential(id) => ConnectionError::CredentialNotFound { id },
        }
    }
}

/// Which secrets a connection has stored: what the GUI and MCP may know about them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretFlags {
    pub has_password: bool,
    pub has_key: bool,
    pub has_passphrase: bool,
}

impl SecretFlags {
    /// Whether a secret of `kind` is stored.
    #[must_use]
    pub fn has(self, kind: SecretKind) -> bool {
        match kind {
            SecretKind::Password => self.has_password,
            SecretKind::Key => self.has_key,
            SecretKind::Passphrase => self.has_passphrase,
        }
    }

    /// The flags of a row of `select_connections!`, which reads them as `has_*` columns.
    pub(crate) fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        let flag = |column: &str| row.try_get::<i64, _>(column).map(|value| value != 0);
        Ok(Self {
            has_password: flag("has_password")?,
            has_key: flag("has_key")?,
            has_passphrase: flag("has_passphrase")?,
        })
    }
}

/// The public half of a private key stored with a connection, kept in clear for display. MCP
/// sees `{"type", "fingerprint", "comment", "stored": true}`; the public key itself is for the
/// window only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredKey {
    /// The key type as SSH names it (`ssh-ed25519`, `ssh-rsa`, ...).
    #[serde(rename = "type")]
    pub key_type: String,
    /// `SHA256:<base64>` exactly as `ssh-keygen -lf` prints it.
    pub fingerprint: String,
    pub comment: Option<String>,
    /// Always `true` here: the key itself is in the vault, not only a path.
    pub stored: bool,
    /// The public key as an `authorized_keys` line, without the comment.
    #[serde(skip)]
    pub public_key: String,
}

impl StoredKey {
    /// The stored key of a row of `select_connections!`; `None` without one.
    pub(crate) fn from_row(row: &SqliteRow) -> sqlx::Result<Option<Self>> {
        let Some(key_type) = row.try_get::<Option<String>, _>("key_type")? else {
            return Ok(None);
        };
        Ok(Some(Self {
            key_type,
            fingerprint: row
                .try_get::<Option<String>, _>("key_fingerprint")?
                .unwrap_or_default(),
            comment: row.try_get("key_comment")?,
            stored: true,
            public_key: row
                .try_get::<Option<String>, _>("key_public")?
                .unwrap_or_default(),
        }))
    }
}

impl Db {
    /// The ciphertext of `kind` stored for connection `id`; `None` when none is stored.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::ConnectionNotFound`], or the read fails.
    pub async fn connection_secret(
        &self,
        id: ConnectionId,
        kind: SecretKind,
    ) -> TreeResult<Option<Vec<u8>>> {
        self.stored_secret(SecretOwner::Connection(id), kind).await
    }

    /// The ciphertext of `kind` stored for `owner`; `None` when none is stored.
    ///
    /// # Errors
    ///
    /// The owner missing ([`ConnectionError::ConnectionNotFound`],
    /// [`ConnectionError::CredentialNotFound`]), or the read fails.
    pub async fn stored_secret(
        &self,
        owner: SecretOwner,
        kind: SecretKind,
    ) -> TreeResult<Option<Vec<u8>>> {
        let row: Option<(Option<Vec<u8>>,)> =
            sqlx::query_as(AssertSqlSafe(kind.select(owner.table())))
                .bind(owner.row())
                .fetch_optional(self.readers())
                .await?;
        row.map(|(value,)| value).ok_or_else(|| owner.missing())
    }

    /// Stores the ciphertext `value` as the connection's secret of `kind`, or removes that
    /// secret with `None`. Answers the connection with its new flags.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::ConnectionNotFound`]; [`crate::DbError::VaultChanged`] when a value is
    /// to be stored and the vault key this process holds is no longer the vault's
    /// ([`Db::hold_vault_key`]); or the write fails.
    pub async fn set_connection_secret(
        &self,
        id: ConnectionId,
        kind: SecretKind,
        value: Option<&[u8]>,
    ) -> TreeResult<Connection> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        if value.is_some() {
            self.check_vault_key(&mut tx).await?;
        }
        write_secret(&mut tx, SecretOwner::Connection(id), kind, value).await?;
        let connection = fetch(&mut tx, id).await?;
        tx.commit().await?;
        Ok(connection)
    }

    /// Stores a private key taken into the vault: its ciphertext `key`, its public half `info`
    /// and, when given, the ciphertext of its passphrase - in one transaction. Answers the
    /// connection with its new flags and key.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::ConnectionNotFound`], [`crate::DbError::VaultChanged`] as for
    /// [`Db::set_connection_secret`], or the write fails.
    pub async fn store_connection_key(
        &self,
        id: ConnectionId,
        key: &[u8],
        info: &StoredKey,
        passphrase: Option<&[u8]>,
    ) -> TreeResult<Connection> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        self.check_vault_key(&mut tx).await?;
        write_key(&mut tx, SecretOwner::Connection(id), key, info, passphrase).await?;
        let connection = fetch(&mut tx, id).await?;
        tx.commit().await?;
        Ok(connection)
    }
}

/// Stores the ciphertext `value` as `owner`'s secret of `kind`, or removes it with `None`, on the
/// writer inside its transaction.
pub(crate) async fn write_secret(
    connection: &mut SqliteConnection,
    owner: SecretOwner,
    kind: SecretKind,
    value: Option<&[u8]>,
) -> TreeResult<()> {
    let done = sqlx::query(AssertSqlSafe(kind.update(owner.table())))
        .bind(value.map(<[u8]>::to_vec))
        .bind(owner.row())
        .execute(&mut *connection)
        .await?;
    if done.rows_affected() == 0 {
        return Err(owner.missing());
    }
    Ok(())
}

/// Stores a private key's ciphertext `key`, its public half `info` and, when given, its
/// passphrase's ciphertext with `owner`, on the writer inside its transaction; without a
/// passphrase the stored one stays.
pub(crate) async fn write_key(
    connection: &mut SqliteConnection,
    owner: SecretOwner,
    key: &[u8],
    info: &StoredKey,
    passphrase: Option<&[u8]>,
) -> TreeResult<()> {
    let done = sqlx::query(AssertSqlSafe(format!(
        "UPDATE {} SET key_enc = ?, key_type = ?, key_fingerprint = ?, \
         key_comment = ?, key_public = ?, passphrase_enc = ifnull(?, passphrase_enc), \
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        owner.table()
    )))
    .bind(key.to_vec())
    .bind(&info.key_type)
    .bind(&info.fingerprint)
    .bind(&info.comment)
    .bind(&info.public_key)
    .bind(passphrase.map(<[u8]>::to_vec))
    .bind(owner.row())
    .execute(&mut *connection)
    .await?;
    if done.rows_affected() == 0 {
        return Err(owner.missing());
    }
    Ok(())
}

#[cfg(test)]
#[path = "secrets_tests.rs"]
mod tests;
