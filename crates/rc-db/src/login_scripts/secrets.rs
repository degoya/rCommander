//! A login script's own secrets: rc-vault ciphertexts in `login_script_secrets.value_enc`, with
//! `"login_script_secrets/value_enc/<id>"` as associated data. The database moves bytes; the
//! caller seals the value for the row it is given, inside the transaction that writes it.

use sqlx::Connection as _;

use super::{LoginScriptError, LoginScriptId, ScriptResult, ScriptSecretLabel, require_script};
use crate::Db;

/// The table of a script's secrets; the first part of their associated data.
pub const SCRIPT_SECRETS_TABLE: &str = "login_script_secrets";

/// The column that holds a script secret's ciphertext.
pub const SCRIPT_SECRET_COLUMN: &str = "value_enc";

impl Db {
    /// The ciphertext of secret `id` of `script`; `None` when the script has no such secret.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn login_script_secret(
        &self,
        script: LoginScriptId,
        id: i64,
    ) -> ScriptResult<Option<Vec<u8>>> {
        Ok(sqlx::query_scalar(
            "SELECT value_enc FROM login_script_secrets WHERE id = ? AND script_id = ?",
        )
        .bind(id)
        .bind(script.0)
        .fetch_optional(self.readers())
        .await?)
    }

    /// Stores a secret under `label` with `script`, replacing the value of a secret with that
    /// label (ignoring ASCII case). `seal` gets the row id for the associated data and answers
    /// the ciphertext; when it fails nothing is written and its error comes back inside `Ok`.
    ///
    /// # Errors
    ///
    /// The script missing, the label blank, [`crate::DbError::VaultChanged`] when the vault key
    /// this process holds is no longer the vault's ([`Db::hold_vault_key`]), or the write fails.
    pub async fn store_login_script_secret<E>(
        &self,
        script: LoginScriptId,
        label: &str,
        seal: impl FnOnce(&str) -> Result<Vec<u8>, E>,
    ) -> ScriptResult<Result<ScriptSecretLabel, E>> {
        let label = label.trim();
        if label.is_empty() {
            return Err(LoginScriptError::LabelEmpty);
        }
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        require_script(&mut tx, script).await?;
        self.check_vault_key(&mut tx).await?;
        let existing: Option<(i64, String)> = sqlx::query_as(
            "SELECT id, label FROM login_script_secrets \
             WHERE script_id = ? AND label = ? COLLATE NOCASE",
        )
        .bind(script.0)
        .bind(label)
        .fetch_optional(&mut *tx)
        .await?;
        let (id, label) = match existing {
            Some(kept) => kept,
            None => {
                // An empty value until it is sealed below, in the same transaction.
                let done = sqlx::query(
                    "INSERT INTO login_script_secrets (script_id, label, value_enc) \
                     VALUES (?, ?, zeroblob(0))",
                )
                .bind(script.0)
                .bind(label)
                .execute(&mut *tx)
                .await?;
                (done.last_insert_rowid(), label.to_owned())
            }
        };
        let cipher = match seal(&id.to_string()) {
            Ok(cipher) => cipher,
            Err(error) => return Ok(Err(error)),
        };
        sqlx::query(
            "UPDATE login_script_secrets SET value_enc = ?, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(cipher)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Ok(ScriptSecretLabel { id, label }))
    }

    /// Removes secret `id` from `script`.
    ///
    /// # Errors
    ///
    /// [`LoginScriptError::SecretNotFound`], or the write fails.
    pub async fn delete_login_script_secret(
        &self,
        script: LoginScriptId,
        id: i64,
    ) -> ScriptResult<()> {
        let mut writer = self.writer().await;
        let done = sqlx::query("DELETE FROM login_script_secrets WHERE id = ? AND script_id = ?")
            .bind(id)
            .bind(script.0)
            .execute(&mut *writer)
            .await?;
        if done.rows_affected() == 0 {
            return Err(LoginScriptError::SecretNotFound { script, id });
        }
        Ok(())
    }
}
