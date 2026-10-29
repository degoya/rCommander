//! One encrypted value: `v1 | nonce | ciphertext`, XChaCha20-Poly1305.
//!
//! XChaCha20-Poly1305 rather than ChaCha20-Poly1305: its 192-bit nonce can be drawn at random
//! for every write without counting writes or fearing a repeat, which a 96-bit random nonce
//! only allows up to some billions of values under one key. The associated data binds a value
//! to its field (`FieldRef`), so the tag fails when the value is moved.

use chacha20poly1305::{
    KeyInit as _, XChaCha20Poly1305, XNonce,
    aead::{Aead as _, Payload},
};
use rand::Rng as _;
use zeroize::Zeroizing;

use crate::{Result, VaultError, kdf::VaultKey};

/// The format byte of every value this build writes.
pub(crate) const FORMAT_V1: u8 = 1;

const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;

/// The plaintext of the verifier in `vault_meta`; that it decrypts proves the password.
pub(crate) const VERIFIER_PLAINTEXT: &[u8] = b"rCommander vault verifier v1";

/// The verifier's associated data, in the form of a field's.
pub(crate) const VERIFIER_AAD: &[u8] = b"vault_meta/verifier/1";

fn cipher(key: &VaultKey) -> Result<XChaCha20Poly1305> {
    // A 32-byte key always fits; the error branch is unreachable but not a panic.
    XChaCha20Poly1305::new_from_slice(key.bytes()).map_err(|_| VaultError::BadParameters)
}

/// `plaintext` encrypted under `key` for the field whose associated data is `aad`, with a fresh
/// random nonce.
///
/// # Errors
///
/// The cipher refuses (cannot happen with a 32-byte key and a value below 256 GiB).
pub(crate) fn seal(key: &VaultKey, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    let mut nonce = [0_u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce);
    let ciphertext = cipher(key)?
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| VaultError::Undecryptable)?;
    let mut value = Vec::with_capacity(1 + NONCE_LEN + ciphertext.len());
    value.push(FORMAT_V1);
    value.extend_from_slice(&nonce);
    value.extend_from_slice(&ciphertext);
    Ok(value)
}

/// The plaintext of `value` (written by [`seal`]) for the field whose associated data is `aad`.
///
/// # Errors
///
/// [`VaultError::Malformed`] for a value too short or of another format,
/// [`VaultError::Undecryptable`] when the tag fails: another key, another field, damaged bytes.
pub(crate) fn open(key: &VaultKey, aad: &[u8], value: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    let Some((&format, rest)) = value.split_first() else {
        return Err(VaultError::Malformed);
    };
    if format != FORMAT_V1 || rest.len() < NONCE_LEN + TAG_LEN {
        return Err(VaultError::Malformed);
    }
    let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
    let nonce = <&XNonce>::try_from(nonce).map_err(|_| VaultError::Malformed)?;
    cipher(key)?
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| VaultError::Undecryptable)
}

#[cfg(test)]
#[path = "aead_tests.rs"]
mod tests;
