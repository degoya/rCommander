//! The `.rcplug` package: its members, their size limits, the digest its signature covers, and
//! reading one with every member bounded.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/lib.rs` (`VerifiedPackage`,
//! `VerifyError`, the member constants) and `verifier.rs` (`read_archive`, `package_digest`,
//! `read_member`) at commit 26c8592a6. A `.rcplug` is a ZIP of exactly `manifest.toml` (1 MiB),
//! `component.wasm` (64 MiB), `signature.ed25519` (outside development mode) and
//! `locales/<lang>.json` (256 KiB each, at most 16); anything else in it refuses it
//! (`docs/plugins.md`, "Packages, signing and trust").

use std::io::{Read, Seek};

use anyhow::{Context, Result, bail};

use crate::{
    MAX_LOCALE_BYTES, MAX_LOCALE_FILES, MAX_MANIFEST_BYTES, PluginManifest, locale_member_language,
    sign::DigestBuilder,
};

/// The file name extension of a package.
pub const PACKAGE_EXTENSION: &str = "rcplug";
pub const MANIFEST_MEMBER: &str = "manifest.toml";
pub const COMPONENT_MEMBER: &str = "component.wasm";
pub const SIGNATURE_MEMBER: &str = "signature.ed25519";
/// The largest component a package holds.
pub const MAX_COMPONENT_BYTES: u64 = 64 * 1024 * 1024;
/// The largest signature member: base64 of 64 bytes is 88 characters.
pub const MAX_SIGNATURE_BYTES: u64 = 1024;
/// The largest package file read at all: every member at its limit, and room for the ZIP.
pub const MAX_PACKAGE_BYTES: u64 = 80 * 1024 * 1024;

/// A package's members, read and checked: what an install writes and a start loads.
#[derive(Clone)]
pub struct VerifiedPackage {
    pub manifest: PluginManifest,
    pub manifest_bytes: Vec<u8>,
    pub component: Vec<u8>,
    /// `None` for an unsigned package, which only development mode accepts.
    pub signature: Option<Vec<u8>>,
    /// `(language, raw JSON)`, sorted by language, exactly as signed.
    pub locales: Vec<(String, Vec<u8>)>,
}

impl VerifiedPackage {
    /// [`package_digest`] of its members.
    #[must_use]
    pub fn digest(&self) -> [u8; 32] {
        package_digest(&self.manifest_bytes, &self.component, &self.locales)
    }
}

impl std::fmt::Debug for VerifiedPackage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VerifiedPackage")
            .field("id", &self.manifest.id)
            .field("version", &self.manifest.version)
            .field("component_bytes", &self.component.len())
            .field("signed", &self.signature.is_some())
            .finish_non_exhaustive()
    }
}

/// Why a package was refused, keeping the trust decision a person can make apart from the
/// refusals nobody can override.
#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    /// Consistent and signed by the key its manifest names, which this installation does not
    /// trust yet: the preview shows the fingerprint and asks (trust on first use).
    #[error("untrusted signing key {key_id} (fingerprint {fingerprint})")]
    UntrustedKey {
        key_id: String,
        /// The base64 key the package declares, recorded once a person confirms it.
        public_key: String,
        fingerprint: String,
        name: String,
        version: String,
    },
    /// Anything else: a broken or tampered package, a withdrawn key or digest, an import the
    /// manifest does not grant.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl VerifyError {
    /// The stable code: `plugin_key_untrusted` for the trust decision, the [`crate::Refusal`]'s
    /// for every other refusal.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::UntrustedKey { .. } => "plugin_key_untrusted",
            Self::Other(error) => crate::refusal_code(error),
        }
    }
}

/// The members of a package: manifest, component, signature, locales.
pub(crate) type PackageParts = (Vec<u8>, Vec<u8>, Option<Vec<u8>>, Vec<(String, Vec<u8>)>);

/// Reads the members of a `.rcplug`, refusing any other member and any member over its limit.
pub(crate) fn read_archive<R: Read + Seek>(reader: R) -> Result<PackageParts> {
    let mut archive = zip::ZipArchive::new(reader).context("read .rcplug archive")?;
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    let mut languages = Vec::new();
    for name in &names {
        match name.as_str() {
            MANIFEST_MEMBER | COMPONENT_MEMBER | SIGNATURE_MEMBER => {}
            other => match locale_member_language(other) {
                Some(language) => languages.push(language.to_owned()),
                None => bail!("unexpected member {other} in .rcplug archive"),
            },
        }
    }
    if languages.len() > MAX_LOCALE_FILES {
        bail!("plugin ships more than {MAX_LOCALE_FILES} locale files");
    }
    languages.sort();
    let manifest = read_member(&mut archive, MANIFEST_MEMBER, MAX_MANIFEST_BYTES as u64)?;
    let component = read_member(&mut archive, COMPONENT_MEMBER, MAX_COMPONENT_BYTES)?;
    let signature = read_optional_member(&mut archive, SIGNATURE_MEMBER, MAX_SIGNATURE_BYTES)?;
    let mut locales = Vec::with_capacity(languages.len());
    for language in languages {
        let member = format!("locales/{language}.json");
        let bytes = read_member(&mut archive, &member, MAX_LOCALE_BYTES)?;
        locales.push((language, bytes));
    }
    Ok((manifest, component, signature, locales))
}

/// The exact payload a package signature covers: every member length-prefixed, the locales
/// folded in by name, so a translation is as tamper-evident as the component.
#[must_use]
pub fn package_digest(
    manifest: &[u8],
    component: &[u8],
    locales: &[(String, Vec<u8>)],
) -> [u8; 32] {
    let mut builder = DigestBuilder::new();
    builder.field(manifest);
    builder.field(component);
    builder.named_fields(locales);
    builder.finish()
}

fn read_member<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>> {
    read_optional_member(archive, name, limit)?.with_context(|| format!("missing {name}"))
}

fn read_optional_member<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<Option<Vec<u8>>> {
    let member = match archive.by_name(name) {
        Ok(member) => member,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if member.size() > limit {
        bail!("plugin member {name} exceeds its size limit");
    }
    // The header's size is the archive's word; the read is held to the limit whatever it says.
    let mut bytes = Vec::with_capacity(usize::try_from(member.size()).unwrap_or(0));
    member.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        bail!("plugin member {name} exceeds its size limit");
    }
    Ok(Some(bytes))
}

#[cfg(test)]
#[path = "package_tests.rs"]
mod tests;
