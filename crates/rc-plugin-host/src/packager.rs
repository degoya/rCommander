//! Builds and signs `.rcplug` packages, and makes the key pairs that sign them: the library behind
//! `rcommander plugin keygen|package|verify` (the subcommands are RC-100-12's).
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/packager.rs` at commit
//! 26c8592a6: the same order - manifest, locales and the component's imports checked before
//! anything is signed - and the same round trip through the verifier, so a broken package never
//! leaves the packager. Adapted: rCommander's manifest and member names, the plugin id as the
//! locales' slug.

use std::io::{Cursor, Write};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{
    SigningKey,
    pkcs8::{DecodePrivateKey, EncodePrivateKey, spki::der::pem::LineEnding},
};
use rand::Rng as _;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::{
    VerifiedPackage,
    package::{COMPONENT_MEMBER, MANIFEST_MEMBER, SIGNATURE_MEMBER, VerifyError},
    package_digest, parse_manifest,
    sign::{decode_public_key, sign_detached},
    validate_locales,
    verifier::{PluginVerifier, check_component},
};

/// A fresh signing key with its PEM and its base64 public key.
pub struct GeneratedKey {
    pub signing_key: SigningKey,
    /// PKCS#8 PEM: the private half, kept out of every repository.
    pub private_pem: String,
    /// What a manifest's `public_key` names.
    pub public_base64: String,
}

impl std::fmt::Debug for GeneratedKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GeneratedKey")
            .field("public_base64", &self.public_base64)
            .finish_non_exhaustive()
    }
}

/// Generates an Ed25519 key pair for plugin releases (`rcommander plugin keygen`).
///
/// # Errors
///
/// The key does not encode as PKCS#8, which an Ed25519 key always does.
pub fn generate_signing_key() -> Result<GeneratedKey> {
    let mut seed = [0_u8; 32];
    rand::rng().fill_bytes(&mut seed);
    let signing_key = SigningKey::from_bytes(&seed);
    let private_pem = signing_key
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|error| anyhow::anyhow!("encode the key as PKCS#8: {error}"))?
        .to_string();
    let public_base64 = public_key_base64(&signing_key);
    Ok(GeneratedKey {
        signing_key,
        private_pem,
        public_base64,
    })
}

/// Reads a PKCS#8 PEM private key as [`generate_signing_key`] or `openssl genpkey -algorithm
/// ed25519` writes it.
///
/// # Errors
///
/// The text is no Ed25519 PKCS#8 key.
pub fn load_signing_key_pem(pem: &str) -> Result<SigningKey> {
    SigningKey::from_pkcs8_pem(pem.trim())
        .map_err(|error| anyhow::anyhow!("parse Ed25519 PKCS#8 private key: {error}"))
}

/// The base64 public key of a signing key, as a manifest's `public_key` names it.
#[must_use]
pub fn public_key_base64(key: &SigningKey) -> String {
    STANDARD.encode(key.verifying_key().to_bytes())
}

/// Checks manifest, locales and component, signs the package digest - or, with `key` `None`,
/// leaves it unsigned for development mode - and verifies the result
/// (`rcommander plugin package`).
///
/// `locales` holds `(language, raw JSON)`; they are signed with the rest, so a translation cannot
/// be swapped after release.
///
/// # Errors
///
/// The manifest, a locale or the component is refused, the key is not the manifest's, or the
/// package does not verify.
pub fn package_plugin(
    manifest_bytes: &[u8],
    component: &[u8],
    locales: &[(String, Vec<u8>)],
    key: Option<&SigningKey>,
) -> Result<Vec<u8>> {
    let manifest = parse_manifest(manifest_bytes)?;
    validate_locales(&manifest.id, locales)?;
    check_component(&manifest, component)?;
    if let Some(key) = key
        && decode_public_key(&manifest.public_key)? != key.verifying_key()
    {
        bail!("manifest public_key does not match the signing key");
    }
    let signature =
        key.map(|key| sign_detached(key, &package_digest(manifest_bytes, component, locales)));
    let mut sorted: Vec<&(String, Vec<u8>)> = locales.iter().collect();
    sorted.sort_by(|left, right| left.0.cmp(&right.0));

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    writer.start_file(MANIFEST_MEMBER, options)?;
    writer.write_all(manifest_bytes)?;
    writer.start_file(COMPONENT_MEMBER, options)?;
    writer.write_all(component)?;
    if let Some(signature) = &signature {
        writer.start_file(SIGNATURE_MEMBER, options)?;
        writer.write_all(signature.as_bytes())?;
    }
    for (language, bytes) in sorted {
        writer.start_file(format!("locales/{language}.json"), options)?;
        writer.write_all(bytes)?;
    }
    let archive = writer.finish()?.into_inner();
    let verifier = PluginVerifier::new(key.is_none());
    if let Some(key) = key {
        verifier.trust_key(manifest.key_id.clone(), key.verifying_key())?;
    }
    verifier
        .verify_bytes(&archive)
        .map_err(|error| match error {
            VerifyError::Other(error) => error.context("packaged plugin failed verification"),
            other => anyhow::anyhow!("packaged plugin failed verification: {other}"),
        })?;
    Ok(archive)
}

/// Verifies a package file as an installation does and reports what it is
/// (`rcommander plugin verify`): trusted are the compiled-in roots and `trusted` (key id, base64
/// key); an unknown key answers [`VerifyError::UntrustedKey`].
///
/// # Errors
///
/// The package does not verify.
pub fn verify_package(
    bytes: &[u8],
    trusted: &[(String, String)],
    development_mode: bool,
) -> Result<VerifiedPackage, VerifyError> {
    let verifier = PluginVerifier::with_roots(development_mode)?;
    for (key_id, public_key) in trusted {
        verifier
            .trust_key_base64(key_id.clone(), public_key)
            .context("a trusted key")?;
    }
    verifier.verify_bytes(bytes)
}

#[cfg(test)]
#[path = "packager_tests.rs"]
mod tests;
