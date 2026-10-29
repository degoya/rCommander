//! Login scripts as the database holds them (`login_scripts`, `login_script_secrets`, migration
//! 0009), and which script a connection runs (`connections.login_script_id`).
//!
//! The steps are opaque here: the JSON of rc-remote's `login_script::Step` and its schema
//! version, which rc-remote reads and checks. A script's secrets are rc-vault ciphertexts in
//! `login_script_secrets.value_enc`; a script read from here carries only their ids and labels,
//! never a value. Each write is one `BEGIN IMMEDIATE` transaction on the serialized writer.

mod secrets;

use std::fmt;

use serde::{Deserialize, Serialize};
use sqlx::{Connection as _, SqliteConnection};

use crate::{
    Db, DbError,
    connections::{Connection, ConnectionError, ConnectionId, fetch as fetch_connection},
};

pub use secrets::{SCRIPT_SECRET_COLUMN, SCRIPT_SECRETS_TABLE};

/// A login script's id. Never reused once deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LoginScriptId(pub i64);

impl fmt::Display for LoginScriptId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A stored script. Its secrets by id and label only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginScriptRecord {
    pub id: LoginScriptId,
    pub name: String,
    pub schema_version: i64,
    /// The steps' JSON.
    pub steps: String,
    pub step_timeout_ms: u64,
    pub total_timeout_ms: u64,
    pub secrets: Vec<ScriptSecretLabel>,
    /// The connections that run it.
    pub connections: Vec<ConnectionId>,
    pub created_at: String,
    pub updated_at: String,
}

/// A secret stored with a script, as anyone may see it: never its value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptSecretLabel {
    pub id: i64,
    pub label: String,
}

/// What a script is written with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoginScriptFields<'a> {
    pub name: &'a str,
    pub schema_version: i64,
    pub steps: &'a str,
    pub step_timeout_ms: u64,
    pub total_timeout_ms: u64,
}

/// What deleting a script removed with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginScriptDeletion {
    /// Its stored secrets.
    pub secrets: u64,
    /// The connections that ran it and now run none.
    pub connections: u64,
}

/// A refused or failed change to the login scripts, each with a stable code.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LoginScriptError {
    #[error("there is no login script with id {id}")]
    NotFound { id: LoginScriptId },
    #[error("login script {script} has no secret with id {id}")]
    SecretNotFound { script: LoginScriptId, id: i64 },
    #[error("a name must not be empty")]
    NameEmpty,
    #[error("the name {name:?} is already taken")]
    NameTaken { name: String },
    #[error("a secret's label must not be empty")]
    LabelEmpty,
    #[error("there is no connection with id {id}")]
    ConnectionNotFound { id: ConnectionId },
    #[error(transparent)]
    Db(#[from] DbError),
}

impl LoginScriptError {
    /// Every code, in declaration order.
    pub const ALL_CODES: &[&str] = &[
        "login_script_not_found",
        "login_script_secret_not_found",
        "name_empty",
        "name_taken",
        "login_script_label_empty",
        "connection_not_found",
        "database_error",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound { .. } => "login_script_not_found",
            Self::SecretNotFound { .. } => "login_script_secret_not_found",
            Self::NameEmpty => "name_empty",
            Self::NameTaken { .. } => "name_taken",
            Self::LabelEmpty => "login_script_label_empty",
            Self::ConnectionNotFound { .. } => "connection_not_found",
            Self::Db(_) => "database_error",
        }
    }
}

impl From<sqlx::Error> for LoginScriptError {
    fn from(error: sqlx::Error) -> Self {
        Self::Db(DbError::Sqlx(error))
    }
}

/// The result of every login-script call.
pub type ScriptResult<T> = Result<T, LoginScriptError>;

type ScriptRow = (i64, String, i64, String, i64, i64, String, String);

const SELECT_SCRIPT: &str = "SELECT id, name, schema_version, steps, step_timeout_ms, \
     total_timeout_ms, created_at, updated_at FROM login_scripts WHERE id = ?";

impl Db {
    /// Every login script, by name.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn login_scripts(&self) -> ScriptResult<Vec<LoginScriptRecord>> {
        let mut reader = self.readers().acquire().await?;
        let ids: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM login_scripts ORDER BY name COLLATE NOCASE, id")
                .fetch_all(&mut *reader)
                .await?;
        let mut scripts = Vec::with_capacity(ids.len());
        for id in ids {
            scripts.push(fetch(&mut reader, LoginScriptId(id)).await?);
        }
        Ok(scripts)
    }

    /// The login script `id`.
    ///
    /// # Errors
    ///
    /// [`LoginScriptError::NotFound`], or the read fails.
    pub async fn login_script(&self, id: LoginScriptId) -> ScriptResult<LoginScriptRecord> {
        let mut reader = self.readers().acquire().await?;
        fetch(&mut reader, id).await
    }

    /// Adds a script.
    ///
    /// # Errors
    ///
    /// The name blank or taken, or the write fails.
    pub async fn create_login_script(
        &self,
        fields: &LoginScriptFields<'_>,
    ) -> ScriptResult<LoginScriptRecord> {
        let name = checked_name(fields.name)?;
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        require_free_name(&mut tx, name, None).await?;
        let done = sqlx::query(
            "INSERT INTO login_scripts \
             (name, schema_version, steps, step_timeout_ms, total_timeout_ms) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(name)
        .bind(fields.schema_version)
        .bind(fields.steps)
        .bind(millis(fields.step_timeout_ms))
        .bind(millis(fields.total_timeout_ms))
        .execute(&mut *tx)
        .await
        .map_err(|error| write_error(error, name))?;
        let script = fetch(&mut tx, LoginScriptId(done.last_insert_rowid())).await?;
        tx.commit().await?;
        Ok(script)
    }

    /// Replaces a script's name, steps and timeouts.
    ///
    /// # Errors
    ///
    /// The script missing, the name blank or taken, or the write fails.
    pub async fn update_login_script(
        &self,
        id: LoginScriptId,
        fields: &LoginScriptFields<'_>,
    ) -> ScriptResult<LoginScriptRecord> {
        let name = checked_name(fields.name)?;
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        require_free_name(&mut tx, name, Some(id)).await?;
        let done = sqlx::query(
            "UPDATE login_scripts SET name = ?, schema_version = ?, steps = ?, \
             step_timeout_ms = ?, total_timeout_ms = ?, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(name)
        .bind(fields.schema_version)
        .bind(fields.steps)
        .bind(millis(fields.step_timeout_ms))
        .bind(millis(fields.total_timeout_ms))
        .bind(id.0)
        .execute(&mut *tx)
        .await
        .map_err(|error| write_error(error, name))?;
        if done.rows_affected() == 0 {
            return Err(LoginScriptError::NotFound { id });
        }
        let script = fetch(&mut tx, id).await?;
        tx.commit().await?;
        Ok(script)
    }

    /// Deletes a script with its secrets; the connections that ran it run none.
    ///
    /// # Errors
    ///
    /// [`LoginScriptError::NotFound`], or the write fails.
    pub async fn delete_login_script(
        &self,
        id: LoginScriptId,
    ) -> ScriptResult<LoginScriptDeletion> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let script = fetch(&mut tx, id).await?;
        sqlx::query("DELETE FROM login_scripts WHERE id = ?")
            .bind(id.0)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(LoginScriptDeletion {
            secrets: count(script.secrets.len()),
            connections: count(script.connections.len()),
        })
    }

    /// Sets the script `connection` runs after it logged in, or none.
    ///
    /// # Errors
    ///
    /// The connection or the script missing, or the write fails.
    pub async fn set_connection_login_script(
        &self,
        connection: ConnectionId,
        script: Option<LoginScriptId>,
    ) -> ScriptResult<Connection> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        write_login_script(&mut tx, connection, script).await?;
        let connection =
            fetch_connection(&mut tx, connection)
                .await
                .map_err(|error| match error {
                    ConnectionError::Db(error) => LoginScriptError::Db(error),
                    _ => LoginScriptError::ConnectionNotFound { id: connection },
                })?;
        tx.commit().await?;
        Ok(connection)
    }
}

/// Sets the script `connection` runs, or none, on the writer inside its transaction.
pub(crate) async fn write_login_script(
    connection: &mut SqliteConnection,
    id: ConnectionId,
    script: Option<LoginScriptId>,
) -> ScriptResult<()> {
    if let Some(script) = script {
        require_script(connection, script).await?;
    }
    let done = sqlx::query(
        "UPDATE connections SET login_script_id = ?, \
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
    )
    .bind(script.map(|script| script.0))
    .bind(id.0)
    .execute(&mut *connection)
    .await?;
    if done.rows_affected() == 0 {
        return Err(LoginScriptError::ConnectionNotFound { id });
    }
    Ok(())
}

/// The script `id` with its secrets' labels and its connections, read on `connection`.
async fn fetch(
    connection: &mut SqliteConnection,
    id: LoginScriptId,
) -> ScriptResult<LoginScriptRecord> {
    let row: ScriptRow = sqlx::query_as(SELECT_SCRIPT)
        .bind(id.0)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or(LoginScriptError::NotFound { id })?;
    let secrets: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, label FROM login_script_secrets WHERE script_id = ? \
         ORDER BY label COLLATE NOCASE, id",
    )
    .bind(id.0)
    .fetch_all(&mut *connection)
    .await?;
    let connections: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM connections WHERE login_script_id = ? ORDER BY id")
            .bind(id.0)
            .fetch_all(&mut *connection)
            .await?;
    let (id, name, schema_version, steps, step_timeout, total_timeout, created_at, updated_at) =
        row;
    Ok(LoginScriptRecord {
        id: LoginScriptId(id),
        name,
        schema_version,
        steps,
        step_timeout_ms: u64::try_from(step_timeout).unwrap_or_default(),
        total_timeout_ms: u64::try_from(total_timeout).unwrap_or_default(),
        secrets: secrets
            .into_iter()
            .map(|(id, label)| ScriptSecretLabel { id, label })
            .collect(),
        connections: connections.into_iter().map(ConnectionId).collect(),
        created_at,
        updated_at,
    })
}

/// Fails with [`LoginScriptError::NotFound`] unless the script exists.
pub(crate) async fn require_script(
    connection: &mut SqliteConnection,
    id: LoginScriptId,
) -> ScriptResult<()> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM login_scripts WHERE id = ?)")
            .bind(id.0)
            .fetch_one(&mut *connection)
            .await?;
    if exists {
        Ok(())
    } else {
        Err(LoginScriptError::NotFound { id })
    }
}

/// Fails with [`LoginScriptError::NameTaken`] when another script than `except` has `name`,
/// ignoring ASCII case like the unique index.
async fn require_free_name(
    connection: &mut SqliteConnection,
    name: &str,
    except: Option<LoginScriptId>,
) -> ScriptResult<()> {
    let taken: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM login_scripts WHERE name = ? COLLATE NOCASE AND id <> ?)",
    )
    .bind(name)
    .bind(except.map_or(0, |id| id.0))
    .fetch_one(&mut *connection)
    .await?;
    if taken {
        return Err(LoginScriptError::NameTaken {
            name: name.to_owned(),
        });
    }
    Ok(())
}

/// `name` trimmed; refused when blank.
fn checked_name(name: &str) -> ScriptResult<&str> {
    let name = name.trim();
    if name.is_empty() {
        Err(LoginScriptError::NameEmpty)
    } else {
        Ok(name)
    }
}

/// A unique-index violation is the name taken (another process can race the check).
fn write_error(error: sqlx::Error, name: &str) -> LoginScriptError {
    if let sqlx::Error::Database(database) = &error
        && database.is_unique_violation()
    {
        return LoginScriptError::NameTaken {
            name: name.to_owned(),
        };
    }
    error.into()
}

/// Milliseconds as SQLite stores them; rc-remote bounds them far below `i64::MAX`.
fn millis(ms: u64) -> i64 {
    i64::try_from(ms).unwrap_or(i64::MAX)
}

fn count(len: usize) -> u64 {
    u64::try_from(len).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "login_scripts_tests.rs"]
mod tests;
