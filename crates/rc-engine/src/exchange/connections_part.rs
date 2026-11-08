//! The connections part of an export (RC-080-19): the connections with their marks, folders,
//! login scripts and secrets, and since RC-110-03 their jump host - by the mark of its exported
//! connection, never by an id of this installation - and their local port forwards.

use serde::{Deserialize, Serialize};

use super::{
    ExchangeError, ExportSecret,
    format::{items, text},
    sources::ExportedSource,
};
use crate::{ConnectionDraft, Step};

/// The connections and the login scripts they run.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionsPart {
    pub connections: Vec<ExportedConnection>,
    pub login_scripts: Vec<ExportedScript>,
}

/// One connection: its mark (`source`, `key`), the folders it sits in from the root down, its
/// fields, its login script by name and, only with secrets, its secrets.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportedConnection {
    pub source: String,
    pub key: String,
    #[serde(default)]
    pub folders: Vec<String>,
    pub connection: ConnectionDraft,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_script: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<ExportSecret>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key: Option<ExportSecret>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<ExportSecret>,
    /// Secrets taken from a source instead (RC-100-34): references, never values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<ExportedSource>,
    /// The jump host, by the mark of its own entry in the file (RC-110-03); one the file does not
    /// hold, nor this installation, comes in as none, with a warning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jump: Option<ExportedMark>,
    /// The local port forwards (RC-110-03); they come in stopped and do not start by themselves
    /// unless they did there.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forwards: Vec<ExportedForward>,
}

/// An exported connection's mark: its `source` and `key`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportedMark {
    pub source: String,
    pub key: String,
}

/// A local port forward of an exported connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportedForward {
    pub bind_address: String,
    pub bind_port: u16,
    pub target_host: String,
    pub target_port: u16,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ExportedConnection {
    /// Whether it carries a secret.
    #[must_use]
    pub fn has_secrets(&self) -> bool {
        self.password.is_some() || self.private_key.is_some() || self.passphrase.is_some()
    }
}

/// A login script; its steps name its secrets by the ids of the machine it came from, which
/// `secrets` tells by label.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportedScript {
    pub name: String,
    pub steps: Vec<Step>,
    pub step_timeout_ms: u64,
    pub total_timeout_ms: u64,
    #[serde(default)]
    pub secrets: Vec<ExportedScriptSecret>,
}

/// A script's secret: its id there and its label, its value only with secrets.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportedScriptSecret {
    pub id: i64,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<ExportSecret>,
}

pub(super) fn check_connection(connection: &ExportedConnection) -> Result<(), ExchangeError> {
    items(connection.folders.len(), "a connection's folders")?;
    let draft = &connection.connection;
    let fields = [
        Some(&connection.source),
        Some(&connection.key),
        Some(&draft.name),
        Some(&draft.host),
        draft.username.as_ref(),
        draft.key_path.as_ref(),
        draft.start_directory.as_ref(),
        draft.notes.as_ref(),
        connection.login_script.as_ref(),
    ];
    for field in fields.into_iter().flatten().chain(&connection.folders) {
        text(field, "a connection")?;
    }
    for secret in [
        &connection.password,
        &connection.private_key,
        &connection.passphrase,
    ]
    .into_iter()
    .flatten()
    {
        text(secret.expose(), "a secret")?;
    }
    if let Some(jump) = &connection.jump {
        text(&jump.source, "a jump host")?;
        text(&jump.key, "a jump host")?;
    }
    if connection.forwards.len() > rc_db::connections::MAX_FORWARDS {
        return Err(super::format::invalid(
            "a connection: more than 32 port forwards",
        ));
    }
    for forward in &connection.forwards {
        text(&forward.bind_address, "a port forward")?;
        text(&forward.target_host, "a port forward")?;
        if let Some(name) = &forward.name {
            text(name, "a port forward")?;
        }
    }
    Ok(())
}
