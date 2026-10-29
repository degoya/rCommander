//! `PluginVerifier`: the trust store and the checks a package passes before anything runs it -
//! the archive with bounded members, the manifest, the locales, the signature against the
//! content digest, the withdrawals, and the import check of the compile.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/verifier.rs` at commit
//! 26c8592a6: the same order (the signature decides before a byte of the component is parsed), a
//! trusted `key_id` pinned to its key, an unknown key a trust decision rather than a refusal.
//! Adapted: rCommander's manifest and slug (the plugin id), [`RevokedKeys`] revoked by a person in
//! place of the keys a repository withdrew, and a [`Refusal`] with a stable code for each refusal
//! the window names. The compile of `SandboxEngine::compile_component` is the WebAssembly
//! validation and the import check at once, so no separate `wasmparser` pass is made.

use std::{
    io::{Cursor, Read, Seek},
    path::Path,
};

use anyhow::{Context, Result};
use ed25519_dalek::VerifyingKey;
use rc_plugin_types::Failure;

use crate::{
    PluginManifest, check_app_version,
    failure::codes,
    installed::InstalledKeys,
    package::{MAX_PACKAGE_BYTES, PackageParts, VerifiedPackage, VerifyError, read_archive},
    package_digest, parse_manifest,
    revocation::{RevokedDigests, RevokedKeys},
    runtime::SandboxEngine,
    sign::{TrustStore, decode_public_key, key_fingerprint, verify_detached_bytes},
    validate_locales,
};

/// The rCommander version a plugin's `metadata.min_app_version` is compared with.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A refusal nobody can override, with the stable code the window and MCP name it by. Found in an
/// error chain with [`refusal_code`].
#[derive(Debug, thiserror::Error)]
#[error("{reason}")]
pub struct Refusal {
    /// `plugin_invalid`, `plugin_denied`, `plugin_unsigned`, `plugin_signature_invalid`,
    /// `plugin_key_mismatch`, `plugin_key_revoked`, `plugin_digest_revoked`,
    /// `plugin_needs_newer_app`, `plugin_key_changed`; the installer's `plugin_version_installed`
    /// and `plugin_version_in_use`.
    pub code: &'static str,
    pub reason: String,
}

/// Every code a package refusal carries ([`Refusal`], [`crate::VerifyError::code`]).
pub const REFUSAL_CODES: &[&str] = &[
    "plugin_key_untrusted",
    codes::PLUGIN_INVALID,
    codes::PLUGIN_DENIED,
    "plugin_unsigned",
    "plugin_signature_invalid",
    "plugin_key_mismatch",
    "plugin_key_revoked",
    "plugin_digest_revoked",
    "plugin_needs_newer_app",
    "plugin_version_installed",
    "plugin_version_in_use",
    KEY_CHANGED,
];

/// An installed plugin's id under another signing key (RC-080-43).
pub(crate) const KEY_CHANGED: &str = "plugin_key_changed";

fn refuse(code: &'static str, reason: impl Into<String>) -> anyhow::Error {
    Refusal {
        code,
        reason: reason.into(),
    }
    .into()
}

/// The code of the [`Refusal`] in `error`'s chain; `plugin_invalid` for any other failure (an
/// unreadable archive, a manifest that does not parse).
#[must_use]
pub fn refusal_code(error: &anyhow::Error) -> &'static str {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<Refusal>())
        .map_or(codes::PLUGIN_INVALID, |refusal| refusal.code)
}

/// The trust store and the policy packages are verified under. Clones share the keys and the
/// withdrawals.
#[derive(Clone, Debug, Default)]
pub struct PluginVerifier {
    pub(crate) trusted_keys: TrustStore,
    pub(crate) revoked: RevokedDigests,
    pub(crate) revoked_keys: RevokedKeys,
    pub(crate) development_mode: bool,
    /// The keys the installed plugins were published under; empty unless
    /// [`PluginVerifier::with_installed_keys`] set them for a new package.
    pub(crate) installed_keys: InstalledKeys,
}

impl PluginVerifier {
    /// A verifier trusting nothing yet; unsigned packages only in development mode.
    #[must_use]
    pub fn new(development_mode: bool) -> Self {
        Self {
            development_mode,
            ..Self::default()
        }
    }

    /// A verifier trusting the compiled-in roots (`sign::EMBEDDED_KEYS`).
    ///
    /// # Errors
    ///
    /// A root does not decode.
    pub fn with_roots(development_mode: bool) -> Result<Self> {
        Ok(Self {
            trusted_keys: TrustStore::with_roots()?,
            ..Self::new(development_mode)
        })
    }

    #[must_use]
    pub fn development_mode(&self) -> bool {
        self.development_mode
    }

    /// This verifier for a package about to be installed: one whose plugin id is installed under
    /// another signing key is refused (`plugin_key_changed`), before its key is trusted or not.
    #[must_use]
    pub fn with_installed_keys(mut self, installed: InstalledKeys) -> Self {
        self.installed_keys = installed;
        self
    }

    /// Trusts `key` under `key_id`: a root, or a key a person confirmed on first use.
    ///
    /// # Errors
    ///
    /// The key id is empty.
    pub fn trust_key(&self, key_id: String, key: VerifyingKey) -> Result<()> {
        self.trusted_keys
            .trust(key_id, key)
            .context("trust a plugin signing key")
    }

    /// [`PluginVerifier::trust_key`] from the key's base64 form.
    ///
    /// # Errors
    ///
    /// The key does not decode.
    pub fn trust_key_base64(&self, key_id: String, encoded: &str) -> Result<()> {
        self.trust_key(key_id, decode_public_key(encoded)?)
    }

    /// Whether `key_id` is trusted, and for which key.
    ///
    /// # Errors
    ///
    /// The store is poisoned.
    pub fn trusted_key(&self, key_id: &str) -> Result<Option<VerifyingKey>> {
        self.trusted_keys.key(key_id)
    }

    /// The withdrawn digests, shared with every clone.
    #[must_use]
    pub fn revoked_digests(&self) -> &RevokedDigests {
        &self.revoked
    }

    /// The revoked keys, shared with every clone.
    #[must_use]
    pub fn revoked_keys(&self) -> &RevokedKeys {
        &self.revoked_keys
    }

    /// Reads and verifies one package file.
    ///
    /// # Errors
    ///
    /// The file cannot be read or is larger than any package; see [`PluginVerifier::verify_bytes`].
    pub fn verify_file(&self, path: &Path) -> Result<VerifiedPackage, VerifyError> {
        let size = std::fs::metadata(path)
            .with_context(|| format!("open plugin package {}", path.display()))?
            .len();
        if size > MAX_PACKAGE_BYTES {
            return Err(refuse(
                codes::PLUGIN_INVALID,
                "the package is larger than any plugin",
            )
            .into());
        }
        let file = std::fs::File::open(path)
            .with_context(|| format!("open plugin package {}", path.display()))?;
        self.verify_reader(file)
    }

    /// Verifies a package in memory.
    ///
    /// # Errors
    ///
    /// [`VerifyError::UntrustedKey`] for a consistent package by a key nobody confirmed; a
    /// [`Refusal`] in [`VerifyError::Other`] for everything else.
    pub fn verify_bytes(&self, bytes: &[u8]) -> Result<VerifiedPackage, VerifyError> {
        self.verify_reader(Cursor::new(bytes))
    }

    fn verify_reader<R: Read + Seek>(&self, reader: R) -> Result<VerifiedPackage, VerifyError> {
        let parts = read_archive(reader)
            .map_err(|error| refuse(codes::PLUGIN_INVALID, format!("{error:#}")))?;
        self.verify_parts(parts)
    }

    /// Everything a package passes before it may run: [`PluginVerifier::verify_contents`], then
    /// the compile, which refuses an import the manifest does not grant.
    pub(crate) fn verify_parts(&self, parts: PackageParts) -> Result<VerifiedPackage, VerifyError> {
        let package = self.verify_contents(parts)?;
        check_component(&package.manifest, &package.component)?;
        Ok(package)
    }

    /// Everything the signature decides, without parsing the component: the manifest, the app
    /// version, the locales, the signature, the withdrawals.
    pub(crate) fn verify_contents(
        &self,
        (manifest_bytes, component, signature, locales): PackageParts,
    ) -> Result<VerifiedPackage, VerifyError> {
        let manifest = parse_manifest(&manifest_bytes)
            .map_err(|error| refuse(codes::PLUGIN_INVALID, format!("{error:#}")))?;
        check_app_version(&manifest, APP_VERSION)
            .map_err(|error| refuse("plugin_needs_newer_app", format!("{error:#}")))?;
        validate_locales(&manifest.id, &locales)
            .map_err(|error| refuse(codes::PLUGIN_INVALID, format!("{error:#}")))?;
        let digest = package_digest(&manifest_bytes, &component, &locales);
        // The signature before anything touches the component: it is the one unbounded member.
        match signature.as_deref() {
            Some(encoded) => self.verify_signed(&manifest, &digest, encoded)?,
            None if !self.development_mode => {
                return Err(refuse(
                    "plugin_unsigned",
                    "unsigned plugins require development mode",
                )
                .into());
            }
            None => {
                let fingerprint = decode_public_key(&manifest.public_key)
                    .ok()
                    .map(|key| key_fingerprint(&key));
                self.check_key_unchanged(&manifest, fingerprint.as_deref())?;
                tracing::warn!(
                    plugin = %manifest.name,
                    version = %manifest.version,
                    "an unsigned plugin package is accepted in development mode"
                );
            }
        }
        // After the signature, so a tampered package says so rather than "withdrawn".
        if self.revoked.contains(&digest)? {
            return Err(refuse(
                "plugin_digest_revoked",
                format!(
                    "plugin package {} {} was withdrawn by its content digest",
                    manifest.name, manifest.version
                ),
            )
            .into());
        }
        Ok(VerifiedPackage {
            manifest,
            manifest_bytes,
            component,
            signature,
            locales,
        })
    }

    /// The signature against the manifest's key: a revoked key refused, a trusted `key_id` pinned
    /// to its key, an unknown one checked for consistency and handed to a person.
    fn verify_signed(
        &self,
        manifest: &PluginManifest,
        digest: &[u8; 32],
        encoded: &[u8],
    ) -> Result<(), VerifyError> {
        let declared = decode_public_key(&manifest.public_key)
            .map_err(|error| refuse(codes::PLUGIN_INVALID, format!("{error:#}")))?;
        let fingerprint = key_fingerprint(&declared);
        if self.revoked_keys.contains(&fingerprint)? {
            return Err(refuse(
                "plugin_key_revoked",
                format!("plugin signing key {} was revoked", manifest.key_id),
            )
            .into());
        }
        let checked = |key: &VerifyingKey| {
            verify_detached_bytes(key, digest, encoded)
                .map_err(|error| refuse("plugin_signature_invalid", format!("{error:#}")))
        };
        if self
            .installed_keys
            .changed(&manifest.id, Some(&fingerprint))
        {
            // A tampered package says so first.
            checked(&declared)?;
            self.check_key_unchanged(manifest, Some(&fingerprint))?;
        }
        match self.trusted_keys.key(&manifest.key_id)? {
            Some(trusted) if trusted == declared => Ok(checked(&trusted)?),
            Some(_) => Err(refuse(
                "plugin_key_mismatch",
                format!(
                    "plugin public_key does not match the trusted key for {}",
                    manifest.key_id
                ),
            )
            .into()),
            None => {
                checked(&declared)?;
                Err(VerifyError::UntrustedKey {
                    key_id: manifest.key_id.clone(),
                    public_key: manifest.public_key.clone(),
                    fingerprint,
                    name: manifest.name.clone(),
                    version: manifest.version.clone(),
                })
            }
        }
    }

    /// `plugin_key_changed` when the plugin of `manifest` is installed under a key other than
    /// `fingerprint` (`None`: a key that does not decode).
    fn check_key_unchanged(
        &self,
        manifest: &PluginManifest,
        fingerprint: Option<&str>,
    ) -> Result<(), VerifyError> {
        if self.installed_keys.changed(&manifest.id, fingerprint) {
            return Err(refuse(
                KEY_CHANGED,
                format!(
                    "{} is installed under another signing key; remove it before installing \
                     this package",
                    manifest.name
                ),
            )
            .into());
        }
        Ok(())
    }
}

/// Compiles the component once and refuses an import its manifest does not grant
/// (`plugin_denied`) or a component that is none (`plugin_invalid`).
pub(crate) fn check_component(manifest: &PluginManifest, component: &[u8]) -> Result<()> {
    let sandbox = SandboxEngine::new(manifest.limits, manifest.timeout_milliseconds())
        .map_err(|error| refuse(codes::PLUGIN_INVALID, format!("{error:#}")))?;
    sandbox
        .compile_component(component, manifest)
        .map(|_| ())
        .map_err(|failure: Failure| {
            let code = match failure.code() {
                Some(codes::PLUGIN_DENIED) => codes::PLUGIN_DENIED,
                _ => codes::PLUGIN_INVALID,
            };
            refuse(code, format!("the plugin component is refused: {code}"))
        })
}

#[cfg(test)]
#[path = "verifier_tests.rs"]
mod tests;
