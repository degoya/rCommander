//! Withdrawn package digests and signing keys, and the hex form digests are stored and shown in.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/revocation.rs`
//! (`RevokedDigests`, `WithdrawnKeys`, `format_package_digest`, `parse_package_digest`) at commit
//! 26c8592a6. Two axes, because they answer different questions: a revoked **key** says "nothing
//! this author signs is acceptable", a withdrawn **digest** says "this exact version is bad,
//! whoever signed it" - the one a published-then-withdrawn version needs without taking down every
//! other plugin of its author. Adapted: a key is revoked by a person here (rDownloader's arrive in
//! a repository index), so [`RevokedKeys`] can take one back too. The sets are seeded from the
//! database (`plugin_keys`, `plugin_digest_revocations`) through the state the engine writes.

use std::{
    collections::BTreeSet,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use anyhow::{Result, bail};

/// A set shared between clones: the verifier is cloned into everything that loads plugins, and a
/// withdrawal has to reach all of them at once.
#[derive(Clone, Debug)]
struct SharedSet<T: Ord> {
    inner: Arc<RwLock<BTreeSet<T>>>,
    what: &'static str,
}

impl<T: Ord + Clone> SharedSet<T> {
    fn new(what: &'static str) -> Self {
        Self {
            inner: Arc::default(),
            what,
        }
    }

    fn read(&self) -> Result<RwLockReadGuard<'_, BTreeSet<T>>> {
        self.inner
            .read()
            .map_err(|_| anyhow::anyhow!("{} is poisoned", self.what))
    }

    fn write(&self) -> Result<RwLockWriteGuard<'_, BTreeSet<T>>> {
        self.inner
            .write()
            .map_err(|_| anyhow::anyhow!("{} is poisoned", self.what))
    }
}

/// Withdrawn package digests ([`crate::package_digest`]).
#[derive(Clone, Debug)]
pub struct RevokedDigests(SharedSet<[u8; 32]>);

impl Default for RevokedDigests {
    fn default() -> Self {
        Self(SharedSet::new("plugin revocation list"))
    }
}

impl RevokedDigests {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether this exact package was withdrawn.
    ///
    /// # Errors
    ///
    /// The set is poisoned.
    pub fn contains(&self, digest: &[u8; 32]) -> Result<bool> {
        Ok(self.0.read()?.contains(digest))
    }

    /// Withdraws one digest; whether it was not withdrawn yet.
    ///
    /// # Errors
    ///
    /// The set is poisoned.
    pub fn insert(&self, digest: [u8; 32]) -> Result<bool> {
        Ok(self.0.write()?.insert(digest))
    }

    /// Takes a withdrawal back; whether the digest was withdrawn at all.
    ///
    /// # Errors
    ///
    /// The set is poisoned.
    pub fn remove(&self, digest: &[u8; 32]) -> Result<bool> {
        Ok(self.0.write()?.remove(digest))
    }

    /// Replaces the whole set: the stored rows are the truth after a restart.
    ///
    /// # Errors
    ///
    /// The set is poisoned.
    pub fn replace(&self, digests: impl IntoIterator<Item = [u8; 32]>) -> Result<()> {
        *self.0.write()? = digests.into_iter().collect();
        Ok(())
    }
}

/// Revoked signing keys, by the lower-case hex fingerprint [`crate::key_fingerprint`] writes:
/// refused whether or not anybody trusted them, trust prompt included.
#[derive(Clone, Debug)]
pub struct RevokedKeys(SharedSet<String>);

impl Default for RevokedKeys {
    fn default() -> Self {
        Self(SharedSet::new("revoked key list"))
    }
}

impl RevokedKeys {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the key with this fingerprint is revoked.
    ///
    /// # Errors
    ///
    /// The set is poisoned.
    pub fn contains(&self, fingerprint: &str) -> Result<bool> {
        Ok(self.0.read()?.contains(&fingerprint.to_ascii_lowercase()))
    }

    /// Revokes one key; whether it was not revoked yet.
    ///
    /// # Errors
    ///
    /// The set is poisoned.
    pub fn insert(&self, fingerprint: &str) -> Result<bool> {
        Ok(self.0.write()?.insert(fingerprint.to_ascii_lowercase()))
    }

    /// Replaces the whole set, for the seeding from what is stored.
    ///
    /// # Errors
    ///
    /// The set is poisoned.
    pub fn replace(&self, fingerprints: impl IntoIterator<Item = String>) -> Result<()> {
        *self.0.write()? = fingerprints
            .into_iter()
            .map(|fingerprint| fingerprint.to_ascii_lowercase())
            .collect();
        Ok(())
    }
}

/// A package digest as the 64 lower-case hex characters stored and shown: a person compares it
/// against a published one, as a key fingerprint.
#[must_use]
pub fn format_package_digest(digest: &[u8; 32]) -> String {
    crate::compile_cache::hex(digest)
}

/// The hex form back, refusing anything that is not exactly 32 bytes of hex: a digest that parses
/// into something else would withdraw a package nobody named.
///
/// # Errors
///
/// Not 64 hexadecimal characters.
pub fn parse_package_digest(text: &str) -> Result<[u8; 32]> {
    let text = text.trim();
    if text.len() != 64 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("a package digest is 64 hexadecimal characters");
    }
    let mut digest = [0_u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)?;
    }
    Ok(digest)
}

#[cfg(test)]
#[path = "revocation_tests.rs"]
mod tests;
