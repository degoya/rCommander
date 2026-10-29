//! The framing a package is hashed with before it is signed.
//!
//! Copied from rDownloader's `crates/rd-sign/src/digest.rs` at commit 26c8592a6, unchanged. One
//! rule: **every field is length-prefixed**. Concatenating `"ab" + "c"` and `"a" + "bc"` gives the
//! same bytes, so a hash over a bare concatenation lets an attacker move a member boundary
//! without changing the digest; a big-endian `u64` length before each field removes that freedom.
//!
//! The framing is **frozen**: every `.rcplug` is signed with it and the key that verifies the
//! bundled ones is compiled in. The tests pin it with byte-literal vectors.

use sha2::{Digest, Sha256};

/// Accumulates the length-prefixed payload that gets signed.
///
/// Fields are hashed in call order; [`DigestBuilder::named_fields`] sorts, for input that has no
/// order of its own.
#[derive(Clone, Debug, Default)]
pub struct DigestBuilder {
    hasher: Sha256,
}

impl DigestBuilder {
    /// Starts an empty payload.
    #[must_use]
    pub fn new() -> Self {
        Self {
            hasher: Sha256::new(),
        }
    }

    /// Appends one length-prefixed field.
    pub fn field(&mut self, bytes: &[u8]) -> &mut Self {
        self.hasher.update((bytes.len() as u64).to_be_bytes());
        self.hasher.update(bytes);
        self
    }

    /// Appends a count-prefixed set of `(name, bytes)` pairs, sorted by name: they come from a
    /// directory listing or a map, and a digest that depended on their order would be
    /// reproducible only by accident.
    pub fn named_fields(&mut self, entries: &[(String, Vec<u8>)]) -> &mut Self {
        let mut sorted: Vec<&(String, Vec<u8>)> = entries.iter().collect();
        sorted.sort_by(|left, right| left.0.cmp(&right.0));
        self.hasher.update((sorted.len() as u64).to_be_bytes());
        for (name, bytes) in sorted {
            self.field(name.as_bytes());
            self.field(bytes);
        }
        self
    }

    /// Finishes the payload.
    #[must_use]
    pub fn finish(self) -> [u8; 32] {
        self.hasher.finalize().into()
    }
}

/// Hex SHA-256 of `bytes`, the form every fingerprint takes in the window and the logs.
#[must_use]
pub fn hex_sha256(bytes: &[u8]) -> String {
    crate::compile_cache::hex(&Sha256::digest(bytes))
}

#[cfg(test)]
#[path = "digest_tests.rs"]
mod tests;
