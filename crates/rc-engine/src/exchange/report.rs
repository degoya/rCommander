//! The parts of an export, the rows of an import's preview and what an export wrote. No row
//! carries a secret: a connection says only that it brings some.

use serde::{Deserialize, Serialize};

/// The four parts an export holds, each chosen on its own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportParts {
    /// The settings of the schema, the protected ones among them.
    #[serde(default)]
    pub settings: bool,
    /// The connection tree: folders, connections, login scripts, key paths.
    #[serde(default)]
    pub connections: bool,
    /// The trusted SSH host keys and FTPS certificates.
    #[serde(default)]
    pub trust: bool,
    /// The hotlist and the tabs of both panels.
    #[serde(default)]
    pub hotlist: bool,
}

impl ExportParts {
    /// Every part.
    pub const ALL: Self = Self {
        settings: true,
        connections: true,
        trust: true,
        hotlist: true,
    };

    /// Whether any part is chosen.
    #[must_use]
    pub fn any(self) -> bool {
        self.settings || self.connections || self.trust || self.hotlist
    }
}

/// The part a row belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExchangePart {
    Settings,
    Connections,
    Trust,
    Hotlist,
}

/// What a row is, or what happened to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RowStatus {
    /// Not here yet.
    New,
    /// Here with the same values ("Already there").
    Exists,
    /// Here with other values.
    Changed,
    /// Not importable ("Not imported"): `code` says why - an error code, or the field refused.
    Refused {
        code: String,
    },
    /// Written by this import.
    Created,
    Updated,
    /// Chosen, but its write failed: `code` is the error's.
    Failed {
        code: String,
    },
}

impl RowStatus {
    /// Whether a person can choose the row: it is new or changed, or here and bringing secrets.
    #[must_use]
    pub fn choosable(&self, secrets: bool) -> bool {
        match self {
            Self::New | Self::Changed => true,
            Self::Exists => secrets,
            _ => false,
        }
    }
}

/// One row of an import: a setting, a login script, a connection, a host key, a certificate, a
/// hotlist item or a tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangeRow {
    /// Stable within the file: `setting:<key>`, `script:<name>`, `connection:<source>/<key>`,
    /// `host_key:<host>:<port>:<type>`, `certificate:<host>:<port>`, `hotlist:<n>`, `tab:<n>`.
    pub key: String,
    pub part: ExchangePart,
    /// What the row is: a setting's key, a connection's folders and name, a host.
    pub name: String,
    /// The value here now, where it differs; never a secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old: Option<String>,
    /// The value in the file; never a secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<String>,
    /// Why the row waits for a person's tick rather than being chosen: `protected_setting`,
    /// `host_key_changed`, `certificate_changed`, `endpoint_with_secrets`, `network_location`,
    /// `secret_sources`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    /// It brings a password, a private key, a passphrase or a script's secret.
    #[serde(default)]
    pub secrets: bool,
    pub selected: bool,
    #[serde(flatten)]
    pub status: RowStatus,
}

/// What an import planned (`dry_run`) or did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangeReport {
    pub dry_run: bool,
    /// Whether the file was sealed with an export password.
    pub encrypted: bool,
    /// When the file was written.
    pub created_at: String,
    /// Whether the file holds secrets; an unencrypted one that says no holds none.
    pub secrets: bool,
    /// The parts the file holds.
    pub parts: ExportParts,
    pub rows: Vec<ExchangeRow>,
}

/// What an export wrote: the file and how many of each it holds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportReport {
    pub path: String,
    pub bytes: u64,
    pub encrypted: bool,
    /// Whether passwords, private keys and passphrases went with it.
    pub secrets: bool,
    pub settings: usize,
    pub connections: usize,
    pub login_scripts: usize,
    pub host_keys: usize,
    pub certificates: usize,
    pub hotlist: usize,
    pub tabs: usize,
    /// Hotlist items and tabs on a server or in an archive: they name this machine's
    /// connections and are not exported.
    pub skipped: usize,
}
