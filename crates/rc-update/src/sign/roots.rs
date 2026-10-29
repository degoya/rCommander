//! The update root compiled into the binary, and how it rotates.
//!
//! The root is the bottom of the update's trust chain: nothing verifies it, so it is shipped,
//! never fetched. A table rather than a constant, so a key can rotate with an overlap: the
//! publisher signs with the new key while the old one is still accepted until its `not_after`,
//! and the old entry is dropped a release later. **No build trusts any other key** - no
//! development key, not even in a debug build; tests sign with a throw-away key against a
//! [`TrustStore`] of their own.
//!
//! Copied and adapted from rDownloader's `crates/rd-sign/src/roots.rs` at commit 26c8592a6
//! (RC-100-04): one role instead of four.

use chrono::{DateTime, Utc};

use super::trust::TrustStore;

/// The key id update manifests are signed under (owner, 2026-10-20: a key of its own, not
/// rDownloader's).
pub const UPDATE_KEY_ID: &str = "rcommander-update-v1";

/// What the table holds until the owner's key is generated (`rc-pack keygen --role update`) and
/// its public half pasted below. Never decoded and never trusted: a build that still carries it
/// answers every check with `update.not_configured` - it fails closed.
pub const PLACEHOLDER_KEY: &str = "TODO-coordinator";

/// One compiled-in public key.
#[derive(Clone, Copy, Debug)]
pub struct EmbeddedKey {
    /// The `key_id` manifests signed with it name.
    pub key_id: &'static str,
    /// Base64 Ed25519 public key; empty or [`PLACEHOLDER_KEY`] for "not configured".
    pub public_key: &'static str,
    /// RFC 3339 instant after which the key is no longer accepted, when it rotates out.
    pub not_after: Option<&'static str>,
}

/// Every update root this build ships. A second entry with the new key and a `not_after` on the
/// old one is a rotation (`docs/development.md`, "Publishing updates").
pub const EMBEDDED_KEYS: &[EmbeddedKey] = &[EmbeddedKey {
    key_id: UPDATE_KEY_ID,
    public_key: "MIGym3Glgp+9YnKzXlnMWwbXYqPKIMXidXvyKJING14=",
    not_after: None,
}];

/// One signed manifest its publisher withdrew, refused whoever signed it.
#[derive(Clone, Copy, Debug)]
pub struct RevokedDocument {
    /// `SignedDocument::digest` of the withdrawn document, 64 lowercase hex characters.
    pub digest: &'static str,
}

/// Every manifest this build refuses by its content; compiled in like the root, because a list
/// fetched beside the manifests could be withheld by whoever serves them.
pub const REVOKED_DOCUMENTS: &[RevokedDocument] = &[];

/// The configured, unexpired keys of `table` at `now`.
#[must_use]
pub fn keys_at(table: &[EmbeddedKey], now: DateTime<Utc>) -> Vec<&EmbeddedKey> {
    table
        .iter()
        .filter(|entry| !entry.public_key.is_empty() && entry.public_key != PLACEHOLDER_KEY)
        .filter(|entry| match entry.not_after {
            Some(text) => DateTime::parse_from_rfc3339(text)
                .map(|expiry| now <= expiry.with_timezone(&Utc))
                // A root that cannot be dated is not one to rely on.
                .unwrap_or(false),
            None => true,
        })
        .collect()
}

/// A trust store of the unexpired keys of `keys` with the documents of `revoked` withdrawn.
///
/// # Errors
///
/// A key or a digest of the tables is malformed: skipping it would quietly accept what it
/// withdraws or trust nothing without saying so.
pub fn trust_store_at(
    keys: &[EmbeddedKey],
    revoked: &[RevokedDocument],
    now: DateTime<Utc>,
) -> anyhow::Result<TrustStore> {
    let store = TrustStore::new();
    for entry in keys_at(keys, now) {
        store.trust_base64(entry.key_id.to_owned(), entry.public_key)?;
    }
    for entry in revoked {
        store.revoke_digest(decode_digest(entry.digest)?)?;
    }
    Ok(store)
}

/// A digest of [`REVOKED_DOCUMENTS`].
fn decode_digest(text: &str) -> anyhow::Result<[u8; 32]> {
    let mut digest = [0_u8; 32];
    anyhow::ensure!(
        text.len() == 64 && text.is_ascii(),
        "a revoked digest is 64 hex characters"
    );
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
            .map_err(|_| anyhow::anyhow!("a revoked digest is 64 hex characters"))?;
    }
    Ok(digest)
}

#[cfg(test)]
#[path = "roots_tests.rs"]
mod tests;
