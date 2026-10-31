//! The `.rcexport` file: JSON with a format name and version, and either the content in clear or
//! the content sealed with an export password (`sealed`, the salt and the ciphertext in hex).
//!
//! A file is read as hostile input: at most [`MAX_FILE_BYTES`], unknown fields refused, every
//! list at most [`MAX_ITEMS`] long and every text at most [`MAX_TEXT`] characters
//! ([`Content::check`]) before one row is compared. What a row says - a setting's value, a host,
//! a path - is checked again row by row by the import, which refuses that row alone.

use std::fmt;

use rc_vault::{KdfParams, SealedExport};
use rc_vfs::VfsPath;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use zeroize::Zeroize as _;

use super::{ExchangeError, sources::ExportedSource};
use crate::{ConnectionDraft, PanelId, Step};

/// The format name every file carries.
pub const FORMAT: &str = "rcommander-export";
/// The version this build writes and reads.
pub const FORMAT_VERSION: u32 = 1;
/// The file ending the dialogs suggest.
pub const EXTENSION: &str = "rcexport";
/// The largest file an import reads.
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// The longest list of a part.
pub const MAX_ITEMS: usize = 10_000;
/// The longest text of a field, in characters.
pub const MAX_TEXT: usize = 8192;
/// What every connection mark of an export starts with: `rcexport:<installation id>`.
pub const SOURCE_PREFIX: &str = "rcexport:";

/// The file.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportFile {
    pub format: String,
    pub version: u32,
    /// When it was written, RFC 3339 in UTC.
    pub created_at: String,
    /// The content in clear; `None` when it is sealed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sealed: Option<SealedFile>,
}

/// The sealed content: the key derivation's name and parameters, the salt and `v1 | nonce |
/// ciphertext`, both in hex.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SealedFile {
    pub kdf: String,
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
    pub salt: String,
    pub content: String,
}

impl SealedFile {
    /// The form a sealed export is written in.
    #[must_use]
    pub fn of(sealed: &SealedExport) -> Self {
        Self {
            kdf: sealed.kdf.clone(),
            m_cost_kib: sealed.params.m_cost_kib,
            t_cost: sealed.params.t_cost,
            p_cost: sealed.params.p_cost,
            salt: hex(&sealed.salt),
            content: hex(&sealed.sealed),
        }
    }

    /// The sealed export it holds.
    ///
    /// # Errors
    ///
    /// [`ExchangeError::FileInvalid`] when the salt or the content is not hex.
    pub fn sealed(&self) -> Result<SealedExport, ExchangeError> {
        Ok(SealedExport {
            kdf: self.kdf.clone(),
            params: KdfParams {
                m_cost_kib: self.m_cost_kib,
                t_cost: self.t_cost,
                p_cost: self.p_cost,
            },
            salt: unhex(&self.salt).ok_or_else(|| invalid("the salt is not hex"))?,
            sealed: unhex(&self.content).ok_or_else(|| invalid("the content is not hex"))?,
        })
    }
}

/// What an export holds; a part it does not hold is `None`.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Content {
    /// The installation the file was written on (`rc_db::Db::installation_id`).
    pub origin: String,
    /// Whether passwords, private keys and passphrases are in it.
    pub secrets: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<Vec<ExportedSetting>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connections: Option<ConnectionsPart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust: Option<TrustPart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotlist: Option<HotlistPart>,
    /// The own colour themes, which go with the settings (RC-100-50).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub themes: Option<Vec<super::ExportedTheme>>,
}

/// One setting of the schema and its value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportedSetting {
    pub setting: String,
    pub value: Value,
}

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

/// The trusted SSH host keys and FTPS certificates.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustPart {
    pub host_keys: Vec<ExportedHostKey>,
    pub certificates: Vec<ExportedCertificate>,
}

/// A host key; its fingerprint is computed again from `public_key` when it is read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportedHostKey {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub public_key: String,
}

/// A trusted certificate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportedCertificate {
    pub host: String,
    pub port: u16,
    pub fingerprint: String,
    pub subject: String,
    pub issuer: String,
    pub not_after: i64,
}

/// The hotlist and the tabs of both panels.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HotlistPart {
    pub items: Vec<ExportedHotlistItem>,
    pub tabs: Vec<ExportedTab>,
}

/// A hotlist entry, or a hotlist folder (no `location`), under the folders `folders`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportedHotlistItem {
    #[serde(default)]
    pub folders: Vec<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<VfsPath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<VfsPath>,
}

/// A folder tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportedTab {
    pub panel: PanelId,
    pub location: VfsPath,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub active: bool,
}

/// A secret in an export's content: zeroized on drop, `[redacted]` in `Debug`. It goes into the
/// content in clear - which is sealed, or the person confirmed that the file holds it in clear.
pub struct ExportSecret(String);

impl ExportSecret {
    #[must_use]
    pub fn new(value: &str) -> Self {
        Self(value.to_owned())
    }

    /// The value, to store in the vault.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for ExportSecret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for ExportSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ExportSecret([redacted])")
    }
}

impl Serialize for ExportSecret {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ExportSecret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self)
    }
}

impl ExportFile {
    /// The file in `bytes`: JSON of this format and version, holding the content in clear or
    /// sealed, never both.
    ///
    /// # Errors
    ///
    /// [`ExchangeError::FileInvalid`], [`ExchangeError::VersionUnknown`].
    pub fn parse(bytes: &[u8]) -> Result<Self, ExchangeError> {
        // The name and version first, so a newer file says so rather than naming a field.
        #[derive(Deserialize)]
        struct Header {
            format: String,
            version: u32,
        }
        let header: Header = serde_json::from_slice(bytes).map_err(|error| json_error(&error))?;
        if header.format != FORMAT {
            return Err(invalid("not an rCommander export"));
        }
        if header.version != FORMAT_VERSION {
            return Err(ExchangeError::VersionUnknown {
                version: header.version,
            });
        }
        let file: Self = serde_json::from_slice(bytes).map_err(|error| json_error(&error))?;
        if file.content.is_some() == file.sealed.is_some() {
            return Err(invalid(
                "the file holds neither or both of content and sealed",
            ));
        }
        Ok(file)
    }
}

impl Content {
    /// The content of a sealed file, once opened.
    ///
    /// # Errors
    ///
    /// [`ExchangeError::FileInvalid`], saying where and never what.
    pub fn parse(bytes: &[u8]) -> Result<Self, ExchangeError> {
        serde_json::from_slice(bytes).map_err(|error| json_error(&error))
    }

    /// The caps of a file read from anywhere: the origin an id, every list at most
    /// [`MAX_ITEMS`] long, every text at most [`MAX_TEXT`] characters.
    ///
    /// # Errors
    ///
    /// [`ExchangeError::FileInvalid`] naming the first part over a cap.
    pub fn check(&self) -> Result<(), ExchangeError> {
        if self.origin.is_empty()
            || self.origin.len() > 64
            || !self.origin.chars().all(|c| c.is_ascii_alphanumeric())
        {
            return Err(invalid("the origin is not an id"));
        }
        if let Some(settings) = &self.settings {
            items(settings.len(), "settings")?;
            for setting in settings {
                text(&setting.setting, "a setting's key")?;
                text(&setting.value.to_string(), "a setting's value")?;
            }
        }
        if let Some(part) = &self.connections {
            items(part.connections.len(), "connections")?;
            items(part.login_scripts.len(), "login scripts")?;
            for connection in &part.connections {
                check_connection(connection)?;
            }
            for script in &part.login_scripts {
                text(&script.name, "a login script's name")?;
                items(script.secrets.len(), "a login script's secrets")?;
                for secret in &script.secrets {
                    text(&secret.label, "a secret's label")?;
                    if let Some(value) = &secret.value {
                        text(value.expose(), "a secret")?;
                    }
                }
            }
        }
        if let Some(trust) = &self.trust {
            items(trust.host_keys.len(), "host keys")?;
            items(trust.certificates.len(), "certificates")?;
            for key in &trust.host_keys {
                for field in [&key.host, &key.algorithm, &key.public_key] {
                    text(field, "a host key")?;
                }
            }
            for cert in &trust.certificates {
                for field in [&cert.host, &cert.fingerprint, &cert.subject, &cert.issuer] {
                    text(field, "a certificate")?;
                }
            }
        }
        if let Some(themes) = &self.themes {
            items(themes.len(), "themes")?;
        }
        if let Some(hotlist) = &self.hotlist {
            items(hotlist.items.len(), "hotlist")?;
            items(hotlist.tabs.len(), "tabs")?;
            for item in &hotlist.items {
                items(item.folders.len(), "a hotlist item's folders")?;
                for name in item.folders.iter().chain([&item.name]) {
                    text(name, "a hotlist name")?;
                }
            }
        }
        Ok(())
    }
}

fn check_connection(connection: &ExportedConnection) -> Result<(), ExchangeError> {
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
    Ok(())
}

fn items(len: usize, what: &str) -> Result<(), ExchangeError> {
    if len > MAX_ITEMS {
        return Err(invalid(&format!("{what}: more than {MAX_ITEMS} entries")));
    }
    Ok(())
}

fn text(value: &str, what: &str) -> Result<(), ExchangeError> {
    if value.chars().count() > MAX_TEXT || value.contains('\0') {
        return Err(invalid(&format!("{what}: too long or not text")));
    }
    Ok(())
}

/// Where the JSON went wrong, never what it said there: a value can be a secret.
fn json_error(error: &serde_json::Error) -> ExchangeError {
    invalid(&format!(
        "not valid at line {} column {}",
        error.line(),
        error.column()
    ))
}

pub(crate) fn invalid(reason: &str) -> ExchangeError {
    ExchangeError::FileInvalid {
        reason: reason.to_owned(),
    }
}

/// `bytes` as lower-case hex.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    text
}

/// The bytes of hex `text`, either case; `None` for an odd length or another character.
#[must_use]
pub fn unhex(text: &str) -> Option<Vec<u8>> {
    let digits = text.as_bytes();
    let (pairs, []) = digits.as_chunks::<2>() else {
        return None;
    };
    let value = |digit: u8| {
        char::from(digit)
            .to_digit(16)
            .and_then(|v| u8::try_from(v).ok())
    };
    pairs
        .iter()
        .map(|&[high, low]| Some((value(high)? << 4) | value(low)?))
        .collect()
}

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;
