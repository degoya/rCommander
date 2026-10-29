//! The plugin signing keys compiled into the binary, and how they rotate.
//!
//! Copied and adapted from rDownloader's `crates/rd-sign/src/roots.rs` at commit 26c8592a6: a
//! table rather than one constant, so a key can be rotated with an overlap window (`not_after`),
//! and a root that cannot be rewritten on disk. Adapted: the plugin role only (rCommander signs
//! no update manifest or repository index yet), and `not_after` in Unix seconds, so no date
//! parser is needed for a table this small.
//!
//! The release key `rcommander-plugin-v1` (decision 7, made 2026-10-29 with `rc-pack keygen
//! --role plugin`) signs the bundled plugins of a release; the development key `rcommander-dev-v1`
//! stays a root for locally signed packages until it is retired. Neither private half enters the
//! repository: they live outside it, with whoever packages the bundled plugins
//! (`docs/security/plugins.md`, "Keys"). A package either signs is trusted without a question -
//! the bundled plugins beside the executable are.

use std::time::{SystemTime, UNIX_EPOCH};

use super::trust::TrustStore;

/// One compiled-in public key.
#[derive(Clone, Copy, Debug)]
pub struct EmbeddedKey {
    /// The `key_id` a manifest signed with it names.
    pub key_id: &'static str,
    /// Base64 Ed25519 public key; empty means "not configured in this build".
    pub public_key: &'static str,
    /// Unix seconds after which the key is no longer accepted, when it is rotated out.
    pub not_after: Option<u64>,
}

/// The development key the bundled plugins are signed with until the release key exists.
pub const DEVELOPMENT_KEY_ID: &str = "rcommander-dev-v1";

/// The release key the bundled plugins of a release are signed with (RC-100-32, decision 7).
pub(crate) const RELEASE_KEY_ID: &str = "rcommander-plugin-v1";

/// Every plugin root this build ships: the release key, and the development key for locally
/// signed packages until it is retired.
pub const EMBEDDED_KEYS: &[EmbeddedKey] = &[
    EmbeddedKey {
        key_id: RELEASE_KEY_ID,
        public_key: "B/mKOjG9t2sF5ikC0sJH6uzMg5TGQpXtwkGU8qLJMP4=",
        not_after: None,
    },
    EmbeddedKey {
        key_id: DEVELOPMENT_KEY_ID,
        public_key: "B0zGe1KstAvHnny52BYAl7rrW3l5SVnBy8Fmf8NVkt0=",
        not_after: None,
    },
];

/// The configured, unexpired roots at the current instant.
#[must_use]
pub fn root_keys() -> Vec<&'static EmbeddedKey> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    root_keys_at(EMBEDDED_KEYS, now)
}

/// The configured roots of `table` still valid at `now` (Unix seconds); an entry without a key is
/// skipped rather than reported.
#[must_use]
pub fn root_keys_at(table: &'static [EmbeddedKey], now: u64) -> Vec<&'static EmbeddedKey> {
    table
        .iter()
        .filter(|entry| !entry.public_key.is_empty())
        .filter(|entry| entry.not_after.is_none_or(|end| now <= end))
        .collect()
}

impl TrustStore {
    /// A store holding every configured root.
    ///
    /// # Errors
    ///
    /// A root does not decode: the build is broken, and nothing is trusted rather than less.
    pub fn with_roots() -> anyhow::Result<Self> {
        let store = Self::new();
        for entry in root_keys() {
            store.trust_base64(entry.key_id.to_owned(), entry.public_key)?;
        }
        Ok(store)
    }
}

#[cfg(test)]
#[path = "roots_tests.rs"]
mod tests;
