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
    /// The seed of the one-time codes a server asks for as a second factor (RC-060-08): an
    /// `otpauth://totp/...` URI or a bare Base32 secret, as pasted.
    TotpSeed,
}

impl SecretKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 4] = [Self::Password, Self::Key, Self::Passphrase, Self::TotpSeed];

    /// The column that holds this kind's ciphertext.
    #[must_use]
    pub fn column(self) -> &'static str {
        match self {
            Self::Password => "password_enc",
            Self::Key => "key_enc",
            Self::Passphrase => "passphrase_enc",
            Self::TotpSeed => "totp_seed_enc",
        }
    }

    /// The name serde and the i18n keys use.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Key => "key",
            Self::Passphrase => "passphrase",
            Self::TotpSeed => "totp_seed",
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
            ", key_type = NULL, key_fingerprint = NULL, key_comment = NULL, key_public = NULL, \
             key_certificate = NULL, key_certificate_info = NULL"
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
    /// A one-time code seed (RC-060-08).
    #[serde(default)]
    pub has_totp_seed: bool,
}

impl SecretFlags {
    /// Whether a secret of `kind` is stored.
    #[must_use]
    pub fn has(self, kind: SecretKind) -> bool {
        match kind {
            SecretKind::Password => self.has_password,
            SecretKind::Key => self.has_key,
            SecretKind::Passphrase => self.has_passphrase,
            SecretKind::TotpSeed => self.has_totp_seed,
        }
    }

    /// The flags of a row of `select_connections!`, which reads them as `has_*` columns.
    pub(crate) fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        let flag = |column: &str| row.try_get::<i64, _>(column).map(|value| value != 0);
        Ok(Self {
            has_password: flag("has_password")?,
            has_key: flag("has_key")?,
            has_passphrase: flag("has_passphrase")?,
            has_totp_seed: flag("has_totp_seed")?,
        })
    }
}

/// The public half of a private key stored with a connection, kept in clear for display. MCP
/// sees `{"type", "fingerprint", "comment", "stored": true, "certificate"}`; the public key
/// itself is for the window only.
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
    /// The OpenSSH user certificate a CA signed for this key, when one was taken in with it
    /// (RC-060-09).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate: Option<KeyCertificate>,
}

/// An OpenSSH user certificate stored beside a private key (RC-060-09): public, like the key's
/// public half. MCP and the window see what it says; its `-cert.pub` line is for the login only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyCertificate {
    /// The certificate type as SSH names it (`ssh-ed25519-cert-v01@openssh.com`, ...).
    #[serde(rename = "type")]
    pub cert_type: String,
    /// The key id the CA wrote into it; what a server logs.
    pub key_id: String,
    pub serial: u64,
    /// The names it is valid for; empty leaves the choice to the server.
    pub principals: Vec<String>,
    /// Valid from this Unix time on; 0 is always.
    pub valid_after: u64,
    /// Valid before this Unix time; `None` is forever.
    pub valid_before: Option<u64>,
    /// `SHA256:<base64>` of the CA that signed it.
    pub ca_fingerprint: String,
    /// The certificate as its `-cert.pub` line, what the login sends.
    #[serde(skip)]
    pub openssh: String,
}

/// Whether a certificate is valid at a given time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CertificateValidity {
    Valid,
    NotYetValid,
    Expired,
}

impl KeyCertificate {
    /// Whether it is valid at the Unix time `now`: valid after <= now < valid before, as
    /// OpenSSH's `PROTOCOL.certkeys` has it.
    #[must_use]
    pub fn validity_at(&self, now: u64) -> CertificateValidity {
        if now < self.valid_after {
            CertificateValidity::NotYetValid
        } else if self.valid_before.is_some_and(|before| now >= before) {
            CertificateValidity::Expired
        } else {
            CertificateValidity::Valid
        }
    }

    /// Whether it names `user` among its principals, or names none.
    #[must_use]
    pub fn names(&self, user: &str) -> bool {
        self.principals.is_empty() || self.principals.iter().any(|name| name == user)
    }

    /// The certificate of a row that reads `key_certificate` and `key_certificate_info`; `None`
    /// without one. A summary that does not read is logged and left out: the key stays usable.
    fn from_row(row: &SqliteRow) -> sqlx::Result<Option<Self>> {
        let openssh = row.try_get::<Option<String>, _>("key_certificate")?;
        let info = row.try_get::<Option<String>, _>("key_certificate_info")?;
        let (Some(openssh), Some(info)) = (openssh, info) else {
            return Ok(None);
        };
        match serde_json::from_str::<Self>(&info) {
            Ok(certificate) => Ok(Some(Self {
                openssh,
                ..certificate
            })),
            Err(error) => {
                tracing::warn!(%error, "a stored certificate's summary does not read");
                Ok(None)
            }
        }
    }

    /// The summary as `key_certificate_info` stores it.
    fn info(&self) -> String {
        serde_json::to_string(self).expect("a certificate's summary has only plain fields")
    }
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
            certificate: KeyCertificate::from_row(row)?,
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

    /// The `-cert.pub` line of the certificate stored with `owner`'s private key (RC-060-09);
    /// `None` without one.
    ///
    /// # Errors
    ///
    /// The owner missing, or the read fails.
    pub async fn key_certificate(&self, owner: SecretOwner) -> TreeResult<Option<String>> {
        let row: Option<(Option<String>,)> = sqlx::query_as(AssertSqlSafe(format!(
            "SELECT key_certificate FROM {} WHERE id = ?",
            owner.table()
        )))
        .bind(owner.row())
        .fetch_optional(self.readers())
        .await?;
        row.map(|(value,)| value).ok_or_else(|| owner.missing())
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

/// Stores a private key's ciphertext `key`, its public half `info` with its certificate (none
/// removes a stored one: it belonged to the key replaced) and, when given, its passphrase's
/// ciphertext with `owner`, on the writer inside its transaction; without a passphrase the
/// stored one stays.
pub(crate) async fn write_key(
    connection: &mut SqliteConnection,
    owner: SecretOwner,
    key: &[u8],
    info: &StoredKey,
    passphrase: Option<&[u8]>,
) -> TreeResult<()> {
    let done = sqlx::query(AssertSqlSafe(format!(
        "UPDATE {} SET key_enc = ?, key_type = ?, key_fingerprint = ?, \
         key_comment = ?, key_public = ?, key_certificate = ?, key_certificate_info = ?, \
         passphrase_enc = ifnull(?, passphrase_enc), updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        owner.table()
    )))
    .bind(key.to_vec())
    .bind(&info.key_type)
    .bind(&info.fingerprint)
    .bind(&info.comment)
    .bind(&info.public_key)
    .bind(info.certificate.as_ref().map(|certificate| certificate.openssh.clone()))
    .bind(info.certificate.as_ref().map(KeyCertificate::info))
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
