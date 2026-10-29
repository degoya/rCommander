//! Signatures, freshness and trust for the update manifest: the length-prefixed digest, one
//! signed-document format bound to a domain string, the replay rules and the compiled-in root.
//!
//! Copied and adapted from rDownloader's `crates/rd-sign/src/` (`lib.rs`, `digest.rs`,
//! `envelope.rs`, `replay.rs`, `roots.rs`, `trust.rs`) at commit 26c8592a6 (RC-100-04), reduced to
//! one role: the update root `rcommander-update-v1`. A module of this crate rather than shared with
//! `rc-plugin-host::sign`: the updater must not link Wasmtime, and the plugin host's roots carry a
//! development key no update may ever trust.

mod digest;
mod envelope;
pub mod replay;
mod roots;
mod trust;

pub use digest::{DigestBuilder, hex, hex_sha256};
pub use envelope::{
    ALGORITHM_ED25519, DocumentSignature, SignedDocument, VerifyError, sign_detached,
    sign_document, verify_detached,
};
pub use replay::{Freshness, StaleError};
pub use roots::{
    EMBEDDED_KEYS, EmbeddedKey, PLACEHOLDER_KEY, REVOKED_DOCUMENTS, RevokedDocument, UPDATE_KEY_ID,
    keys_at, trust_store_at,
};
pub use trust::{TrustStore, decode_public_key, key_fingerprint};

pub use ed25519_dalek::{SigningKey, VerifyingKey};
