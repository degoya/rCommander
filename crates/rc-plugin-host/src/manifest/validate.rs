//! `validate_manifest` and its checks, in the order of the refusals an author sees: the
//! compatibility gates first, then identity, the kind's sections, grants, limits, metadata.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/manifest/validate.rs` and
//! `manifest/checks.rs` at commit 26c8592a6 (`validate_metadata`, `bounded_text`), with
//! rCommander's sections in place of the provider and extension checks.

use anyhow::{Context, Result, bail};
use base64::Engine as _;

use super::grants::{NET_HTTP, net_http};
use super::sections::KNOWN_CAPABILITIES;
use super::settings::validate_settings;
use super::{
    MANIFEST_VERSION, ManifestRejection, MatchSection, PluginManifest, PluginMetadata, PluginType,
    SUPPORTED_API_VERSIONS,
};

const MAX_NAME_CHARS: usize = 64;
const MAX_KEY_ID_CHARS: usize = 64;
const MAX_DESCRIPTION_CHARS: usize = 500;
const MAX_AUTHOR_CHARS: usize = 120;
const MAX_LICENSE_CHARS: usize = 64;
const MAX_URL_CHARS: usize = 256;
/// `[match]`: how many extensions and magics one plugin claims, and how long each is.
const MAX_EXTENSIONS: usize = 64;
const MAX_EXTENSION_CHARS: usize = 16;
const MAX_MAGICS: usize = 16;
const MAX_MAGIC_OFFSET: u64 = 4096;
const MAX_MAGIC_BYTES: usize = 32;
/// The contracts before the secrets world (RC-100-33): a secret source never names one.
const BEFORE_SECRETS: [&str; 2] = ["0.1.0", "0.2.0"];

pub(crate) fn validate_manifest(manifest: &PluginManifest) -> Result<()> {
    if manifest.manifest_version != MANIFEST_VERSION {
        return Err(ManifestRejection::Version {
            found: manifest.manifest_version,
            expected: MANIFEST_VERSION,
        }
        .into());
    }
    if let PluginType::Unknown(value) = &manifest.plugin_type {
        return Err(ManifestRejection::Unknown {
            kind: "plugin_type",
            value: value.clone(),
        }
        .into());
    }
    if !SUPPORTED_API_VERSIONS.contains(&manifest.api_version.as_str()) {
        return Err(ManifestRejection::Unknown {
            kind: "api_version",
            value: manifest.api_version.clone(),
        }
        .into());
    }
    if let Some(name) = manifest
        .capabilities
        .declared
        .keys()
        .find(|name| !KNOWN_CAPABILITIES.contains(&name.as_str()))
    {
        return Err(ManifestRejection::Unknown {
            kind: "capability",
            value: name.clone(),
        }
        .into());
    }
    validate_identity(manifest)?;
    validate_kind(manifest)?;
    manifest.limits.check().map_err(anyhow::Error::msg)?;
    validate_settings(&manifest.settings)?;
    validate_metadata(&manifest.metadata)
}

fn validate_identity(manifest: &PluginManifest) -> Result<()> {
    if !is_uuid_v7(&manifest.id) {
        bail!("plugin id must be a lower-case UUIDv7");
    }
    bounded_text("name", &manifest.name, MAX_NAME_CHARS)?;
    if manifest.name.chars().any(char::is_control) {
        bail!("name must not hold control characters");
    }
    let version = semver::Version::parse(&manifest.version)
        .context("plugin version is not semantic versioning")?;
    // The version names a folder of the installation (RC-100-09), so it is the plain text SemVer
    // parsed, nothing a path could make more of.
    if version.to_string() != manifest.version {
        bail!("plugin version is not written the strict SemVer way");
    }
    bounded_text("key_id", &manifest.key_id, MAX_KEY_ID_CHARS)?;
    if !manifest.key_id.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
    }) {
        bail!("key_id may only contain a-z, 0-9, '-', '_' and '.'");
    }
    let key = base64::engine::general_purpose::STANDARD
        .decode(manifest.public_key.as_bytes())
        .context("public_key is not base64")?;
    if key.len() != 32 {
        bail!("public_key is not a 32-byte Ed25519 key");
    }
    Ok(())
}

/// The sections a kind carries, and the grants it may hold.
fn validate_kind(manifest: &PluginManifest) -> Result<()> {
    match manifest.plugin_type {
        PluginType::Viewer => {
            let Some(viewer) = &manifest.viewer else {
                bail!("a viewer manifest needs a [viewer] section");
            };
            if viewer.outputs.is_empty() {
                bail!("[viewer] outputs must name at least one output");
            }
            for (index, output) in viewer.outputs.iter().enumerate() {
                if viewer.outputs[..index].contains(output) {
                    bail!("[viewer] outputs names `{}` twice", output.as_str());
                }
            }
            validate_match(&manifest.matching)?;
            // The viewer world imports no granted interface (docs/plugins.md, "The worlds").
            if let Some(name) = manifest.capabilities.declared.keys().next() {
                bail!("a viewer plugin cannot be granted the capability `{name}`");
            }
        }
        PluginType::Secrets => {
            if BEFORE_SECRETS.contains(&manifest.api_version.as_str()) {
                bail!("a secret source needs api_version 0.3.0 or later");
            }
            // It is asked for a connection's secret, never offered a file.
            if manifest.viewer.is_some()
                || !manifest.matching.extensions.is_empty()
                || !manifest.matching.magic.is_empty()
            {
                bail!("a secret source has no [viewer] section and claims no file in [match]");
            }
            if let Some(name) = manifest
                .capabilities
                .declared
                .keys()
                .find(|name| name.as_str() != NET_HTTP)
            {
                bail!("a secret source cannot be granted the capability `{name}` yet");
            }
            net_http(manifest)?;
        }
        PluginType::Unknown(_) => unreachable!("unknown types are refused before this point"),
    }
    Ok(())
}

fn validate_match(matching: &MatchSection) -> Result<()> {
    if matching.extensions.is_empty() && matching.magic.is_empty() {
        bail!("[match] must name an extension or a magic, or no file is ever offered");
    }
    if matching.extensions.len() > MAX_EXTENSIONS {
        bail!("[match] names more than {MAX_EXTENSIONS} extensions");
    }
    for (index, extension) in matching.extensions.iter().enumerate() {
        if extension.is_empty()
            || extension.len() > MAX_EXTENSION_CHARS
            || !extension.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
            })
            || matching.extensions[..index].contains(extension)
        {
            bail!("[match] extension `{extension}` is not a plain lower-case extension, or twice");
        }
    }
    if matching.magic.len() > MAX_MAGICS {
        bail!("[match] names more than {MAX_MAGICS} magics");
    }
    for magic in &matching.magic {
        let bytes = magic
            .bytes()
            .with_context(|| format!("[match] magic `{}` is not hex bytes", magic.hex))?;
        if bytes.len() > MAX_MAGIC_BYTES || magic.offset > MAX_MAGIC_OFFSET {
            bail!(
                "[match] magic reads at most {MAX_MAGIC_BYTES} bytes within the first {MAX_MAGIC_OFFSET}"
            );
        }
    }
    if matching.max_size == Some(0) {
        bail!("[match] max_size must be greater than zero");
    }
    Ok(())
}

fn validate_metadata(metadata: &PluginMetadata) -> Result<()> {
    bounded_text(
        "metadata.description",
        &metadata.description,
        MAX_DESCRIPTION_CHARS,
    )?;
    bounded_text("metadata.author", &metadata.author, MAX_AUTHOR_CHARS)?;
    if let Some(license) = &metadata.license {
        bounded_text("metadata.license", license, MAX_LICENSE_CHARS)?;
    }
    for (field, value) in [
        ("metadata.homepage", &metadata.homepage),
        ("metadata.support_url", &metadata.support_url),
    ] {
        let Some(value) = value else { continue };
        bounded_text(field, value, MAX_URL_CHARS)?;
        let url = url::Url::parse(value).with_context(|| format!("{field} is not a valid URL"))?;
        if url.scheme() != "https" {
            bail!("{field} must use https");
        }
    }
    if let Some(min_version) = &metadata.min_app_version {
        semver::Version::parse(min_version)
            .context("metadata.min_app_version is not semantic versioning")?;
    }
    Ok(())
}

fn bounded_text(field: &str, value: &str, limit: usize) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{field} is required");
    }
    if value.chars().count() > limit {
        bail!("{field} exceeds {limit} characters");
    }
    Ok(())
}

/// `019d0000-0000-7000-8000-0000000000a1`: lower-case hex, version 7, the RFC 4122 variant.
pub(crate) fn is_uuid_v7(id: &str) -> bool {
    let bytes = id.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(byte),
        })
        && bytes[14] == b'7'
        && matches!(bytes[19], b'8' | b'9' | b'a' | b'b')
}
