//! `manifest.toml` version 1: the complete, self-describing plugin contract.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/manifest.rs` (manifest
//! version 3) at commit 26c8592a6: the same layout and the same reading - at most 1 MiB, unknown
//! keys refused, strings capped, limits within their ceilings, an unknown plugin type, API
//! version or capability refused rather than ignored. rCommander's sections per kind replace
//! `[provider]`, `[transfer]` and `[extension]` (`docs/plugins.md`, "The manifest"). Signing,
//! installation and the version folders are RC-100-09's.
//!
//! Every grant is one entry in `[capabilities]` and one WIT interface in the linker, so a plugin
//! reaches exactly what its manifest names and the install preview can show that same list.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::PluginLimits;

mod complete;
mod grants;
mod sections;
mod settings;
mod validate;

pub use complete::{COMPLETED_MARKER, ManifestDefaults, complete_manifest};
pub use grants::{NET_HTTP, NetHttp, net_http};
pub use sections::{
    Capabilities, KNOWN_CAPABILITIES, MagicManifest, MatchSection, PluginMetadata, PluginType,
    SettingKind, SettingManifest, ViewerSection,
};
pub(crate) use settings::resolve_settings;
pub use settings::{MAX_SETTING_LINE_CHARS, MAX_SETTING_TEXT_CHARS};
pub(crate) use validate::validate_manifest;

/// The only manifest revision this build accepts.
pub const MANIFEST_VERSION: u32 = 1;

/// Versions of `rcommander:plugin` this build links a plugin against: the contract and the ones
/// before it. A viewer's interfaces are the same in each, 0.1.0 without `settings` (RC-100-14);
/// 0.3.0 adds `http` and the secrets world (RC-100-33).
pub const SUPPORTED_API_VERSIONS: &[&str] = &["0.1.0", "0.2.0", rc_plugin_api::API_VERSION];

/// The largest `manifest.toml` read.
pub const MAX_MANIFEST_BYTES: usize = 1024 * 1024;

/// A manifest this build refuses, in the two shapes a person can act on: an outdated package
/// needs an update, an unknown grant is a package this build cannot run at all.
#[derive(Debug, thiserror::Error)]
pub enum ManifestRejection {
    /// Written for another manifest revision; `plugin_manifest_outdated`.
    #[error("unsupported manifest version {found} (expected {expected})")]
    Version { found: u32, expected: u32 },
    /// Names a plugin type, capability or API version this build does not know;
    /// `plugin_capability_unknown`.
    #[error("unknown {kind} `{value}`")]
    Unknown { kind: &'static str, value: String },
}

impl ManifestRejection {
    /// The stable code the window switches on.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Version { .. } => "plugin_manifest_outdated",
            Self::Unknown { .. } => "plugin_capability_unknown",
        }
    }
}

/// The signed description of a plugin, read from its package.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    /// Must equal [`MANIFEST_VERSION`].
    pub manifest_version: u32,
    /// What the plugin is; decides the world it is instantiated against.
    pub plugin_type: PluginType,
    /// The `rcommander:plugin` version the component was built against.
    pub api_version: String,
    /// A UUIDv7, stable across the plugin's versions.
    pub id: String,
    /// The default display name; `locales/<lang>.json` may override it.
    pub name: String,
    /// Strict SemVer.
    pub version: String,
    /// Names the signing key; the trust store's key for this author.
    pub key_id: String,
    /// Base64 Ed25519 public key of `key_id`.
    pub public_key: String,
    /// Which files the plugin is offered.
    #[serde(rename = "match", default)]
    pub matching: MatchSection,
    /// Required for `plugin_type = "viewer"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewer: Option<ViewerSection>,
    #[serde(default)]
    pub capabilities: Capabilities,
    pub metadata: PluginMetadata,
    #[serde(default)]
    pub limits: PluginLimits,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<SettingManifest>,
}

impl PluginManifest {
    /// The compute time of one call: `[limits]`, or the kind's default.
    #[must_use]
    pub fn timeout_milliseconds(&self) -> u64 {
        self.limits
            .timeout_or(self.plugin_type.default_timeout_milliseconds())
    }
}

/// The revision alone, read before the rest so an outdated package says so rather than naming
/// the first key its revision did not have.
#[derive(Deserialize)]
struct Revision {
    #[serde(default)]
    manifest_version: u32,
}

/// Reads and validates one `manifest.toml`.
///
/// A refusal is a [`ManifestRejection`] (find it with `downcast_ref`) or an opaque chain naming
/// the packaging mistake.
pub fn parse_manifest(bytes: &[u8]) -> Result<PluginManifest> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        bail!("manifest.toml exceeds {MAX_MANIFEST_BYTES} bytes");
    }
    let text = std::str::from_utf8(bytes).context("manifest.toml is not UTF-8")?;
    let revision: Revision = toml::from_str(text).context("parse manifest.toml")?;
    if revision.manifest_version != MANIFEST_VERSION {
        return Err(ManifestRejection::Version {
            found: revision.manifest_version,
            expected: MANIFEST_VERSION,
        }
        .into());
    }
    let manifest: PluginManifest = toml::from_str(text).context("parse manifest.toml")?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

/// Refuses a plugin that needs a newer rCommander than `app_version`.
pub fn check_app_version(manifest: &PluginManifest, app_version: &str) -> Result<()> {
    let Some(required) = &manifest.metadata.min_app_version else {
        return Ok(());
    };
    let required = semver::Version::parse(required)
        .context("metadata.min_app_version is not semantic versioning")?;
    let current =
        semver::Version::parse(app_version).context("application version is not semantic")?;
    if current < required {
        bail!("plugin requires rCommander {required} or newer (running {current})");
    }
    Ok(())
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
