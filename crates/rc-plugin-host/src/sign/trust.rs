//! Which signing keys are trusted, by key id.
//!
//! Copied and adapted from rDownloader's `crates/rd-sign/src/trust.rs` at commit 26c8592a6: the
//! same shared store, fingerprint and key decoding. The digest axis of rDownloader's store is the
//! signed documents' and not taken; withdrawn package digests and keys are `revocation.rs`'.

use std::{
    collections::HashMap,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::VerifyingKey;

use super::digest::hex_sha256;

/// Hex SHA-256 of the raw 32-byte public key: what a person compares on first use, rather than 44
/// characters of base64.
#[must_use]
pub fn key_fingerprint(key: &VerifyingKey) -> String {
    hex_sha256(key.as_bytes())
}

/// Decodes a base64 32-byte Ed25519 public key.
///
/// # Errors
///
/// No base64, not 32 bytes, or not a point of the curve.
pub fn decode_public_key(encoded: &str) -> Result<VerifyingKey> {
    let raw = STANDARD
        .decode(encoded.trim())
        .context("decode public key")?;
    let bytes: [u8; 32] = raw
        .try_into()
        .map_err(|_| anyhow::anyhow!("public key must contain 32 bytes"))?;
    VerifyingKey::from_bytes(&bytes).context("invalid Ed25519 public key")
}

/// Trusted keys by key id, shared between clones: a key confirmed on first use has to be visible
/// to every holder at once.
#[derive(Clone, Debug, Default)]
pub struct TrustStore {
    keys: Arc<RwLock<HashMap<String, VerifyingKey>>>,
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
    /// The key id is empty, or the store is poisoned.
    pub fn trust(&self, key_id: String, key: VerifyingKey) -> Result<()> {
        if key_id.trim().is_empty() {
            bail!("key id is empty");
        }
        self.write()?.insert(key_id, key);
        Ok(())
    }

    /// [`TrustStore::trust`] from the key's base64 form.
    ///
    /// # Errors
    ///
    /// The key does not decode, or [`TrustStore::trust`] fails.
    pub fn trust_base64(&self, key_id: String, encoded: &str) -> Result<()> {
        self.trust(key_id, decode_public_key(encoded)?)
    }

    /// Drops a key; whether it was trusted at all.
    ///
    /// # Errors
    ///
    /// The store is poisoned.
    pub fn revoke(&self, key_id: &str) -> Result<bool> {
        Ok(self.write()?.remove(key_id).is_some())
    }

    /// The key trusted under `key_id`.
    ///
    /// # Errors
    ///
    /// The store is poisoned.
    pub fn key(&self, key_id: &str) -> Result<Option<VerifyingKey>> {
        Ok(self.read()?.get(key_id).copied())
    }

    /// Every trusted key id, sorted.
    ///
    /// # Errors
    ///
    /// The store is poisoned.
    #[cfg(test)]
    pub fn key_ids(&self) -> Result<Vec<String>> {
        let mut ids: Vec<String> = self.read()?.keys().cloned().collect();
        ids.sort();
        Ok(ids)
    }

    fn read(&self) -> Result<RwLockReadGuard<'_, HashMap<String, VerifyingKey>>> {
        self.keys
            .read()
            .map_err(|_| anyhow::anyhow!("trust store is poisoned"))
    }

    fn write(&self) -> Result<RwLockWriteGuard<'_, HashMap<String, VerifyingKey>>> {
        self.keys
            .write()
            .map_err(|_| anyhow::anyhow!("trust store is poisoned"))
    }
}

#[cfg(test)]
#[path = "trust_tests.rs"]
mod tests;
