//! Which keys are trusted, and which documents have been withdrawn.
//!
//! Revoking a **digest** says "this exact document is bad, whoever signed it", which a published
//! and then withdrawn manifest needs without taking down everything else its key signed.
//!
//! Copied from rDownloader's `crates/rd-sign/src/trust.rs` at commit 26c8592a6 (RC-100-04),
//! without the runtime key revocation and listing an update never needs.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::VerifyingKey;

use super::digest::hex_sha256;

/// Hex SHA-256 of the raw 32-byte public key, what a person compares against a published one.
#[must_use]
pub fn key_fingerprint(key: &VerifyingKey) -> String {
    hex_sha256(key.as_bytes())
}

/// Decodes a base64 32-byte Ed25519 public key.
///
/// # Errors
///
/// The text is no base64 of 32 bytes, or no Ed25519 point.
pub fn decode_public_key(encoded: &str) -> Result<VerifyingKey> {
    let raw = STANDARD
        .decode(encoded.trim())
        .context("decode public key")?;
    let bytes: [u8; 32] = raw
        .try_into()
        .map_err(|_| anyhow::anyhow!("public key must contain 32 bytes"))?;
    VerifyingKey::from_bytes(&bytes).context("invalid Ed25519 public key")
}

/// Trusted keys and withdrawn documents, shared between clones.
#[derive(Clone, Debug, Default)]
pub struct TrustStore {
    inner: Arc<RwLock<TrustState>>,
}

#[derive(Debug, Default)]
struct TrustState {
    keys: HashMap<String, VerifyingKey>,
    revoked_digests: HashSet<[u8; 32]>,
}

impl TrustStore {
    /// An empty store: nothing is trusted until a key is added.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Trusts `key` under `key_id`.
    ///
    /// # Errors
    ///
    /// The id is empty, or the store is poisoned.
    pub fn trust(&self, key_id: String, key: VerifyingKey) -> Result<()> {
        if key_id.trim().is_empty() {
            bail!("key id is empty");
        }
        self.write()?.keys.insert(key_id, key);
        Ok(())
    }

    /// Trusts a key given in base64.
    ///
    /// # Errors
    ///
    /// As [`decode_public_key`] and [`Self::trust`].
    pub fn trust_base64(&self, key_id: String, encoded: &str) -> Result<()> {
        self.trust(key_id, decode_public_key(encoded)?)
    }

    /// The key trusted under `key_id`, if any.
    ///
    /// # Errors
    ///
    /// The store is poisoned.
    pub fn key(&self, key_id: &str) -> Result<Option<VerifyingKey>> {
        Ok(self.read()?.keys.get(key_id).copied())
    }

    /// Every trusted key id, sorted.
    ///
    /// # Errors
    ///
    /// The store is poisoned.
    pub fn key_ids(&self) -> Result<Vec<String>> {
        let mut ids: Vec<String> = self.read()?.keys.keys().cloned().collect();
        ids.sort();
        Ok(ids)
    }

    /// Withdraws one exact document by its digest, leaving its signing key trusted.
    ///
    /// # Errors
    ///
    /// The store is poisoned.
    pub fn revoke_digest(&self, digest: [u8; 32]) -> Result<()> {
        self.write()?.revoked_digests.insert(digest);
        Ok(())
    }

    /// Whether this exact document has been withdrawn.
    pub(crate) fn is_revoked_digest(&self, digest: &[u8; 32]) -> Result<bool> {
        Ok(self.read()?.revoked_digests.contains(digest))
    }

    fn read(&self) -> Result<RwLockReadGuard<'_, TrustState>> {
        self.inner
            .read()
            .map_err(|_| anyhow::anyhow!("trust store is poisoned"))
    }

    fn write(&self) -> Result<RwLockWriteGuard<'_, TrustState>> {
        self.inner
            .write()
            .map_err(|_| anyhow::anyhow!("trust store is poisoned"))
    }
}

#[cfg(test)]
#[path = "trust_tests.rs"]
mod tests;
