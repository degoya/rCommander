//! The sections of a manifest: the plugin type, `[match]`, `[viewer]`, `[capabilities]`,
//! `[metadata]` and `[[settings]]`.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/manifest/sections.rs` at
//! commit 26c8592a6: `[metadata]` and the capture of unknown capabilities as they were;
//! `[match]` and `[viewer]` instead of `[provider]` and `[extension]` (`docs/plugins.md`, "The
//! manifest"); `[[settings]]` at the top level with `key`, `type`, `values` and `default`.

use std::collections::BTreeMap;

use rc_plugin_api::viewer::Output;
use serde::{Deserialize, Serialize};

/// What a plugin is. The type decides which world it is instantiated against.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PluginType {
    /// Shows a file in Lister and Quick View (`world viewer-plugin`).
    Viewer,
    /// A password manager as the source of a connection's secret (`world secrets-plugin`,
    /// contract 0.3.0, RC-100-33).
    Secrets,
    /// Anything this build does not know - the kinds of later stages included; refused by
    /// validation rather than treated as a viewer.
    Unknown(String),
}

impl PluginType {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Viewer => "viewer",
            Self::Secrets => "secrets",
            Self::Unknown(value) => value,
        }
    }

    /// The compute time of one call when `[limits]` names none.
    #[must_use]
    pub fn default_timeout_milliseconds(&self) -> u64 {
        match self {
            Self::Viewer => crate::limits::DEFAULT_VIEWER_TIMEOUT_MILLISECONDS,
            Self::Secrets => crate::limits::DEFAULT_SECRETS_TIMEOUT_MILLISECONDS,
            Self::Unknown(_) => crate::limits::DEFAULT_TIMEOUT_MILLISECONDS,
        }
    }
}

impl Serialize for PluginType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PluginType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.as_str() {
            "viewer" => Self::Viewer,
            "secrets" => Self::Secrets,
            _ => Self::Unknown(raw),
        })
    }
}

/// `[match]`: which files the plugin is offered, decided by rCommander without running it.
/// Replaces Total Commander's detect strings.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MatchSection {
    /// Lower-case extensions without the dot.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Leading bytes, any one of which claims the file.
    #[serde(default)]
    pub magic: Vec<MagicManifest>,
    /// Larger files are not offered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_size: Option<u64>,
}

/// Bytes at an offset: `{ offset = 0, hex = "25504446" }`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MagicManifest {
    #[serde(default)]
    pub offset: u64,
    pub hex: String,
}

impl MagicManifest {
    /// The bytes `hex` spells; `None` for anything but pairs of hex digits.
    #[must_use]
    pub fn bytes(&self) -> Option<Vec<u8>> {
        if self.hex.is_empty()
            || !self.hex.len().is_multiple_of(2)
            || !self.hex.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return None;
        }
        (0..self.hex.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(self.hex.get(at..at + 2)?, 16).ok())
            .collect()
    }
}

impl MatchSection {
    /// How many leading bytes of a file [`Self::matches`] needs to see.
    #[must_use]
    pub fn head_length(&self) -> usize {
        self.magic
            .iter()
            .map(|magic| {
                usize::try_from(magic.offset)
                    .unwrap_or(usize::MAX)
                    .saturating_add(magic.hex.len() / 2)
            })
            .max()
            .unwrap_or(0)
    }

    /// Whether a file named `file_name` of `size` bytes, starting with `head`, is offered: an
    /// extension or a magic matches, and it is not larger than `max_size`.
    #[must_use]
    pub fn matches(&self, file_name: &str, head: &[u8], size: u64) -> bool {
        if self.max_size.is_some_and(|max| size > max) {
            return false;
        }
        let extension = file_name
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase());
        let by_extension = extension
            .as_deref()
            .is_some_and(|extension| self.extensions.iter().any(|known| known == extension));
        by_extension
            || self.magic.iter().any(|magic| {
                let (Some(bytes), Ok(offset)) = (magic.bytes(), usize::try_from(magic.offset))
                else {
                    return false;
                };
                head.get(offset..offset.saturating_add(bytes.len())) == Some(bytes.as_slice())
            })
    }
}

/// `[viewer]`: the outputs the plugin may offer; `open` naming another is refused.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ViewerSection {
    pub outputs: Vec<Output>,
}

/// The grants a manifest asks for, each one WIT interface.
///
/// Stage 1 knows the names of rDownloader's grants and the new `tool`, so a manifest naming one
/// is told it is not available to its kind, and an unknown name is refused as unknown; their
/// shapes are read with the stages that link them (`docs/plugins.md`, "Capabilities and rights").
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Capabilities {
    #[serde(flatten)]
    pub declared: BTreeMap<String, toml::Value>,
}

/// Every grant a later stage links: `http`, `net`, the secret markers, a password manager's CLI.
pub const KNOWN_CAPABILITIES: [&str; 4] = ["net_http", "net_stream", "secrets", "tool"];

/// Authorship and presentation, shown in the install preview.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginMetadata {
    pub description: String,
    pub author: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_url: Option<String>,
    /// SPDX licence identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// The first rCommander release that runs the plugin; installation is refused below it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_app_version: Option<String>,
}

/// What kind of value a plugin setting holds.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingKind {
    /// One of `values`.
    Choice,
    /// Free text within the host's rule (`manifest/settings.rs`).
    Text,
}

/// `[[settings]]`: one value the person sets on the plugin's page, stored as the rCommander
/// setting `plugin.<id>.<key>` and read by the plugin through `settings.setting` (contract
/// 0.2.0). Replaces TC's configuration dialogs and INI files.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SettingManifest {
    /// Lower-case letters, digits and underscores; its label is the plugin's code
    /// `<slug>.setting.<key>`.
    pub key: String,
    #[serde(rename = "type")]
    pub kind: SettingKind,
    /// A choice's values, in the order offered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    /// Text only: several lines, edited as a text area.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multiline: bool,
    /// What the plugin is told while the person leaves the setting alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// Text only: a key such as a licence key (RC-100-36) - shown hidden on the plugin's page and
    /// never in an MCP answer.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub masked: bool,
}
