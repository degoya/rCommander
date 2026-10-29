//! Login scripts as the engine sees them (RC-050-08): the model of `rc_remote::login_script`,
//! stored in `rcommander.db` (`rc_db::login_scripts`), and where a run gets its secrets.
//!
//! The `login_scripts.*` commands (`commands/login_scripts.rs`) list, read, write and delete
//! scripts and set the script a connection runs; storing or removing a script's own secret is
//! `GuiOnly`, like every secret. No type here carries a secret's value: a `send_secret` step
//! names where the secret is ([`ScriptSecretRef`]), a script lists its secrets by id and label.
//! [`ScriptSecrets`] hands a run (RC-060-03) the values, read through the vault and remembered
//! for the MCP mask, so an echo of one in a terminal screen is scrubbed like any stored secret.

use async_trait::async_trait;
use rc_db::login_scripts::{SCRIPT_SECRET_COLUMN, SCRIPT_SECRETS_TABLE};
use rc_vault::{FieldRef, SecretString, VaultError};
use serde::{Deserialize, Serialize};

pub use rc_db::login_scripts::{
    LoginScriptDeletion, LoginScriptError, LoginScriptId, LoginScriptRecord, ScriptSecretLabel,
};
pub use rc_remote::login_script::{
    LoginScript, Pattern, PatternError, SCHEMA_VERSION, ScriptInvalid, SecretError,
    SecretRef as ScriptSecretRef, SecretSource, Step,
};

use crate::{ConnectionId, Engine, EngineError, SecretKind, SecretRef};

/// A login script as the commands answer it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginScriptInfo {
    pub id: LoginScriptId,
    pub name: String,
    pub steps: Vec<Step>,
    /// What a `wait_for` without its own timeout waits, in milliseconds.
    pub step_timeout_ms: u64,
    /// How long a whole run may take, in milliseconds.
    pub total_timeout_ms: u64,
    /// The script's own secrets, by id and label; never a value.
    pub secrets: Vec<ScriptSecretLabel>,
    /// The connections that run it.
    pub connections: Vec<ConnectionId>,
    pub created_at: String,
    pub updated_at: String,
}

impl LoginScriptInfo {
    /// The script stored as `record`, its steps read again.
    pub(crate) fn from_record(record: LoginScriptRecord) -> Result<Self, ScriptInvalid> {
        Ok(Self {
            steps: rc_remote::login_script::steps_from_json(record.schema_version, &record.steps)?,
            id: record.id,
            name: record.name,
            step_timeout_ms: record.step_timeout_ms,
            total_timeout_ms: record.total_timeout_ms,
            secrets: record.secrets,
            connections: record.connections,
            created_at: record.created_at,
            updated_at: record.updated_at,
        })
    }

    /// The script a run plays.
    #[must_use]
    pub fn script(&self) -> LoginScript {
        LoginScript {
            steps: self.steps.clone(),
            step_timeout_ms: self.step_timeout_ms,
            total_timeout_ms: self.total_timeout_ms,
        }
    }
}

/// A new script: `login_scripts.create`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginScriptDraft {
    pub name: String,
    #[serde(default)]
    pub steps: Vec<Step>,
    /// `rc_remote::login_script::DEFAULT_STEP_TIMEOUT_MS` when `None`.
    #[serde(default)]
    pub step_timeout_ms: Option<u64>,
    /// `rc_remote::login_script::DEFAULT_TOTAL_TIMEOUT_MS` when `None`.
    #[serde(default)]
    pub total_timeout_ms: Option<u64>,
}

/// Fields to change: `login_scripts.update`. `steps` replaces every step.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginScriptPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub steps: Option<Vec<Step>>,
    #[serde(default)]
    pub step_timeout_ms: Option<u64>,
    #[serde(default)]
    pub total_timeout_ms: Option<u64>,
}

/// The secrets of one run of `script`, on `connection` when it runs on one: what
/// `rc_remote::login_script::run_script` reads before it sends anything.
#[derive(Clone)]
pub struct ScriptSecrets {
    engine: Engine,
    script: LoginScriptId,
    connection: Option<ConnectionId>,
}

impl Engine {
    /// Where a run of `script` on `connection` reads its secrets.
    #[must_use]
    pub fn login_script_secrets(
        &self,
        script: LoginScriptId,
        connection: Option<ConnectionId>,
    ) -> ScriptSecrets {
        ScriptSecrets {
            engine: self.clone(),
            script,
            connection,
        }
    }

    /// Secret `id` of `script`, decrypted and remembered for the MCP mask; `None` when the
    /// script has no such secret.
    ///
    /// # Errors
    ///
    /// [`EngineError::VaultLocked`] while the vault is locked; the value does not decrypt; no
    /// database is attached; the database fails.
    pub async fn resolve_script_secret(
        &self,
        script: LoginScriptId,
        id: i64,
    ) -> Result<Option<SecretString>, EngineError> {
        let Some(cipher) = self.db()?.login_script_secret(script, id).await? else {
            return Ok(None);
        };
        let row = id.to_string();
        let field = FieldRef::new(SCRIPT_SECRETS_TABLE, SCRIPT_SECRET_COLUMN, &row);
        let by = crate::secrets::secret_use();
        match self
            .vault_handle()
            .await?
            .decrypt_string_for(&field, &cipher, by)
        {
            Ok(value) => {
                self.remember_secret(&value);
                Ok(Some(value))
            }
            Err(VaultError::Locked) => {
                tracing::debug!(%script, secret = id, "a login script's secret needs the vault");
                Err(EngineError::VaultLocked)
            }
            Err(error) => Err(error.into()),
        }
    }
}

#[async_trait]
impl SecretSource for ScriptSecrets {
    async fn secret(&self, source: ScriptSecretRef) -> Result<SecretString, SecretError> {
        let resolved = match source {
            ScriptSecretRef::ConnectionPassword => {
                let Some(connection) = self.connection else {
                    return Err(SecretError::Missing);
                };
                self.engine
                    .resolve_secret(SecretRef {
                        connection,
                        kind: SecretKind::Password,
                    })
                    .await
            }
            ScriptSecretRef::Stored { id } => {
                self.engine.resolve_script_secret(self.script, id).await
            }
        };
        match resolved {
            Ok(Some(value)) => Ok(value),
            Ok(None) => Err(SecretError::Missing),
            Err(EngineError::VaultLocked) => Err(SecretError::Locked),
            Err(error) => Err(SecretError::Failed { code: error.code() }),
        }
    }
}
