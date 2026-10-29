//! What a package is before anything installs it: name, version, author, kind, the files it
//! claims, what it asks for and where its key stands - against the trust store and, for a
//! plugin installed already, the key it was installed under ([`KeyStatus::Changed`]).
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/preview.rs` at commit
//! 26c8592a6: the same archive the installer reads, the signature proven against the key its own
//! manifest names - so the publisher shown is the one that signed it - and the trust decision
//! reported instead of made. Nothing is written and no component compiled. Adapted: shown in the
//! window's dialog instead of a REST answer; rCommander's permissions (claimed extensions and
//! magics, the viewer's outputs, granted capabilities) instead of the repository index's.

use std::io::Cursor;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    PluginManifest, check_app_version, format_package_digest, key_fingerprint,
    package::{MAX_PACKAGE_BYTES, read_archive},
    package_digest, parse_manifest,
    sign::{decode_public_key, verify_detached_bytes},
    validate_locales,
    verifier::{APP_VERSION, PluginVerifier},
};

/// Where the package's signing key stands on this installation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyStatus {
    /// Installing needs no further decision.
    Trusted,
    /// Installing asks for the key to be confirmed first, by its fingerprint.
    Untrusted,
    /// The key id is trusted for another key: refused.
    Mismatch,
    /// The key was revoked: refused, prompt included.
    Revoked,
    /// No signature: only development mode takes it, marked as such.
    Unsigned,
    /// The plugin is installed under another signing key: refused until the person removes it
    /// (RC-080-43), so the new key never inherits its settings, associations and port.
    Changed,
}

/// What a plugin may do and reach, as the preview and the plugin list show it.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Permissions {
    /// The extensions it claims (`[match] extensions`).
    pub extensions: Vec<String>,
    /// How many leading-byte patterns it claims files by.
    pub magics: usize,
    /// A viewer's outputs (`text`, `highlighted`, ...).
    pub outputs: Vec<String>,
    /// The granted capabilities beyond its kind's own interfaces, each with its target where it
    /// has one (`net_http localhost:8087`), so an update that moves the target asks again;
    /// empty for every viewer.
    pub capabilities: Vec<String>,
}

impl Permissions {
    #[must_use]
    pub fn of(manifest: &PluginManifest) -> Self {
        Self {
            extensions: manifest.matching.extensions.clone(),
            magics: manifest.matching.magic.len(),
            outputs: manifest
                .viewer
                .iter()
                .flat_map(|viewer| {
                    viewer
                        .outputs
                        .iter()
                        .map(|output| output.as_str().to_owned())
                })
                .collect(),
            capabilities: manifest
                .capabilities
                .declared
                .keys()
                .map(|name| match crate::net_http(manifest) {
                    Ok(Some(grant)) if name == crate::NET_HTTP => grant.describe(),
                    _ => name.clone(),
                })
                .collect(),
        }
    }

    /// What `newer` asks for beyond these: shown before an update is confirmed.
    #[must_use]
    pub fn added_by(&self, newer: &Self) -> Vec<String> {
        let mut added: Vec<String> = newer
            .capabilities
            .iter()
            .filter(|name| !self.capabilities.contains(name))
            .cloned()
            .collect();
        added.extend(
            newer
                .extensions
                .iter()
                .filter(|extension| !self.extensions.contains(extension))
                .map(|extension| format!(".{extension}")),
        );
        added
    }
}

/// Everything the install preview shows.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PackagePreview {
    pub id: String,
    pub name: String,
    pub version: String,
    /// `viewer`.
    pub plugin_type: String,
    pub api_version: String,
    pub min_app_version: Option<String>,
    pub description: String,
    pub author: String,
    pub homepage: Option<String>,
    pub license: Option<String>,
    /// [`crate::package_digest`] in 64 lower-case hex characters.
    pub package_digest: String,
    pub size: u64,
    pub key_id: String,
    /// The signing key's fingerprint; `None` for an unsigned package.
    pub fingerprint: Option<String>,
    pub key: KeyStatus,
    pub permissions: Permissions,
    /// This exact package was withdrawn by its digest.
    pub withdrawn: bool,
    /// Why this build cannot run it, when it cannot; the install refuses it for the same reason.
    pub incompatible: Option<String>,
}

impl PackagePreview {
    /// Whether confirming can install anything: not withdrawn, compatible, its key neither
    /// revoked, a mismatch nor changed, and unsigned only in development mode.
    #[must_use]
    pub fn installable(&self, development_mode: bool) -> bool {
        !self.withdrawn
            && self.incompatible.is_none()
            && match self.key {
                KeyStatus::Trusted | KeyStatus::Untrusted => true,
                KeyStatus::Unsigned => development_mode,
                KeyStatus::Mismatch | KeyStatus::Revoked | KeyStatus::Changed => false,
            }
    }
}

/// Reads one `.rcplug` and describes it without installing it.
///
/// Refuses what is no package at all - an unreadable archive, a manifest that does not parse, a
/// signature that does not hold for the key the manifest names. Everything else is a finding: a
/// package for another contract is described and marked incompatible.
///
/// # Errors
///
/// As above.
pub fn preview_package(bytes: &[u8], verifier: &PluginVerifier) -> Result<PackagePreview> {
    let size = u64::try_from(bytes.len())?;
    ensure!(
        size > 0 && size <= MAX_PACKAGE_BYTES,
        "the package is empty or larger than {MAX_PACKAGE_BYTES} bytes"
    );
    let (manifest_bytes, component, signature, locales) = read_archive(Cursor::new(bytes))?;
    let manifest = parse_manifest(&manifest_bytes)?;
    let digest = package_digest(&manifest_bytes, &component, &locales);
    let incompatible = check_app_version(&manifest, APP_VERSION)
        .and_then(|()| validate_locales(&manifest.id, &locales))
        .err()
        .map(|error| format!("{error:#}"));
    let declared = decode_public_key(&manifest.public_key)?;
    let changed = verifier
        .installed_keys
        .changed(&manifest.id, Some(&key_fingerprint(&declared)));
    let (fingerprint, key) = match signature {
        Some(signature) => {
            verify_detached_bytes(&declared, &digest, &signature)
                .context("the package is not signed by the key its manifest names")?;
            let fingerprint = key_fingerprint(&declared);
            let key = if verifier.revoked_keys.contains(&fingerprint)? {
                KeyStatus::Revoked
            } else if changed {
                KeyStatus::Changed
            } else {
                match verifier.trusted_keys.key(&manifest.key_id)? {
                    Some(trusted) if trusted == declared => KeyStatus::Trusted,
                    Some(_) => KeyStatus::Mismatch,
                    None => KeyStatus::Untrusted,
                }
            };
            (Some(fingerprint), key)
        }
        None if changed => (None, KeyStatus::Changed),
        None => (None, KeyStatus::Unsigned),
    };
    Ok(PackagePreview {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        plugin_type: manifest.plugin_type.as_str().to_owned(),
        api_version: manifest.api_version.clone(),
        min_app_version: manifest.metadata.min_app_version.clone(),
        description: manifest.metadata.description.clone(),
        author: manifest.metadata.author.clone(),
        homepage: manifest.metadata.homepage.clone(),
        license: manifest.metadata.license.clone(),
        package_digest: format_package_digest(&digest),
        size,
        key_id: manifest.key_id.clone(),
        fingerprint,
        key,
        permissions: Permissions::of(&manifest),
        withdrawn: verifier.revoked.contains(&digest)?,
        incompatible,
    })
}

#[cfg(test)]
#[path = "preview_tests.rs"]
mod tests;
