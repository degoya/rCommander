//! The signed-document format of the update manifest.
//!
//! **The payload is signed as the bytes it arrived as**, kept verbatim in a `RawValue` and never
//! re-serialised: canonicalising JSON before hashing gives a producer and a verifier room to
//! disagree about what "the same document" is (number formats, key order, escapes, duplicates).
//!
//! **Every signature is bound to a domain string.** The digest covers a label like
//! `rcommander.update-manifest.v1` before the payload, so a signature over one kind of document
//! cannot be lifted onto another that happens to parse.
//!
//! Copied from rDownloader's `crates/rd-sign/src/envelope.rs` at commit 26c8592a6 (RC-100-04);
//! `verify_detached_bytes` and the signer accessor are left out.

use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::value::RawValue;

use super::{digest::DigestBuilder, trust::TrustStore};

/// The only signature algorithm this format admits.
pub const ALGORITHM_ED25519: &str = "ed25519";

/// One detached signature over a document's payload.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DocumentSignature {
    /// Which trusted key is claimed - never the key itself, which would verify against itself.
    pub key_id: String,
    /// Always [`ALGORITHM_ED25519`]; present so a future algorithm is a refused value.
    pub algorithm: String,
    /// Base64 of the raw 64-byte signature.
    pub signature: String,
}

/// A payload plus the signatures over it.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SignedDocument {
    payload: Box<RawValue>,
    signatures: Vec<DocumentSignature>,
}

/// Why a document was not accepted: an unknown key, a signature that does not hold, a withdrawn
/// document, or malformed input - four different answers.
#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("no signature from a trusted key (saw: {key_ids})")]
    UntrustedKey { key_ids: String },
    #[error("signature from {key_id} does not verify")]
    BadSignature { key_id: String },
    #[error("this document has been revoked")]
    Revoked,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl SignedDocument {
    /// Parses a document without verifying anything; [`Self::verify`] is the only way to its
    /// payload.
    ///
    /// # Errors
    ///
    /// The bytes are no signed document.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).context("parse signed document")
    }

    /// The digest this document's signatures are taken over.
    #[must_use]
    pub fn digest(&self, domain: &str) -> [u8; 32] {
        let mut builder = DigestBuilder::new();
        builder.field(domain.as_bytes());
        builder.field(self.payload.get().as_bytes());
        builder.finish()
    }

    /// Verifies against `trust` and returns the parsed payload. One valid signature from a
    /// trusted key is enough.
    ///
    /// # Errors
    ///
    /// See [`VerifyError`].
    pub fn verify<T: DeserializeOwned>(
        &self,
        domain: &str,
        trust: &TrustStore,
    ) -> Result<T, VerifyError> {
        let digest = self.digest(domain);
        if trust.is_revoked_digest(&digest)? {
            return Err(VerifyError::Revoked);
        }
        let mut named_a_trusted_key = None;
        for entry in &self.signatures {
            if entry.algorithm != ALGORITHM_ED25519 {
                continue;
            }
            let Some(key) = trust.key(&entry.key_id)? else {
                continue;
            };
            named_a_trusted_key = Some(entry.key_id.clone());
            if verify_detached(&key, &digest, &entry.signature).is_ok() {
                return serde_json::from_str(self.payload.get())
                    .context("parse signed document payload")
                    .map_err(VerifyError::Other);
            }
        }
        match named_a_trusted_key {
            // A trusted key was named and did not hold: tampering or corruption, not trust.
            Some(key_id) => Err(VerifyError::BadSignature { key_id }),
            None => Err(VerifyError::UntrustedKey {
                key_ids: self
                    .signatures
                    .iter()
                    .map(|entry| entry.key_id.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            }),
        }
    }
}

/// Verifies one base64 Ed25519 signature over `digest`.
///
/// # Errors
///
/// The text is no base64 signature, or it does not verify.
pub fn verify_detached(key: &VerifyingKey, digest: &[u8; 32], encoded: &str) -> Result<()> {
    let raw = STANDARD
        .decode(encoded.trim())
        .context("decode signature")?;
    let signature = Signature::from_slice(&raw).context("invalid Ed25519 signature")?;
    key.verify(digest, &signature)
        .context("signature verification failed")
}

/// Signs `digest` and returns the base64 form this format stores.
#[must_use]
pub fn sign_detached(key: &SigningKey, digest: &[u8; 32]) -> String {
    STANDARD.encode(key.sign(digest).to_bytes())
}

/// Wraps `payload` in a document signed by `key` under `key_id` for `domain`.
///
/// # Errors
///
/// The payload does not serialise to JSON.
pub fn sign_document<T: Serialize>(
    domain: &str,
    key_id: &str,
    key: &SigningKey,
    payload: &T,
) -> Result<SignedDocument> {
    let serialised = serde_json::to_string(payload).context("serialise document payload")?;
    let raw = RawValue::from_string(serialised).context("payload is not a JSON value")?;
    let mut document = SignedDocument {
        payload: raw,
        signatures: Vec::new(),
    };
    let digest = document.digest(domain);
    document.signatures.push(DocumentSignature {
        key_id: key_id.to_owned(),
        algorithm: ALGORITHM_ED25519.to_owned(),
        signature: sign_detached(key, &digest),
    });
    Ok(document)
}

#[cfg(test)]
#[path = "envelope_tests.rs"]
mod tests;
