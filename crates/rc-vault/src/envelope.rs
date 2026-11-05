//! A sealed export (RC-080-19): a file's content encrypted with an export password of its own,
//! never with the vault's key, so the file opens on another machine with another vault.
//!
//! argon2id derives the key from the export password and a fresh salt; the parameters travel
//! beside the ciphertext and are read back only within the import's bounds
//! ([`KdfParams::IMPORT_MAX`], checked before anything is derived, RC-080-68), so a hostile file
//! cannot make an import allocate gigabytes or run for minutes. XChaCha20-Poly1305 seals the
//! content in the vault's value format (`v1 | nonce | ciphertext`) with [`EXPORT_AAD`] as
//! associated data. A wrong password and a changed byte fail
//! alike, with [`VaultError::WrongPassword`]: the tag cannot tell them apart.

use std::fmt;

use crate::{
    MasterPassword, Result, SecretBytes, VaultError, aead,
    kdf::{self, KDF_NAME, KdfParams},
};

/// The associated data of every sealed export: a vault value never opens as an export.
pub const EXPORT_AAD: &[u8] = b"rcommander-export/v1";

/// The sealed content of an export and what it takes to open it again; nothing here is secret.
#[derive(Clone, PartialEq, Eq)]
pub struct SealedExport {
    /// The key derivation's name; `argon2id` is the only one read.
    pub kdf: String,
    pub params: KdfParams,
    pub salt: Vec<u8>,
    /// `v1 | nonce | ciphertext`.
    pub sealed: Vec<u8>,
}

impl fmt::Debug for SealedExport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SealedExport")
            .field("kdf", &self.kdf)
            .field("params", &self.params)
            .field("salt_len", &self.salt.len())
            .field("sealed_len", &self.sealed.len())
            .finish()
    }
}

/// `plaintext` sealed under the key of `password` and a fresh salt, derived with `params`.
/// CPU-bound for about a second with the default parameters; runs on the blocking pool.
///
/// # Errors
///
/// [`VaultError::EmptyPassword`], [`VaultError::BadParameters`] when argon2 refuses `params`,
/// [`VaultError::TooCostly`] above [`KdfParams::IMPORT_MAX`] (no import could open the file),
/// [`VaultError::Interrupted`] when the derivation did not finish.
pub async fn seal_export(
    password: &MasterPassword,
    params: KdfParams,
    plaintext: &[u8],
) -> Result<SealedExport> {
    if password.is_empty() {
        return Err(VaultError::EmptyPassword);
    }
    let params = params.checked()?;
    if params.above(KdfParams::IMPORT_MAX) {
        return Err(VaultError::TooCostly);
    }
    let salt = kdf::new_salt().to_vec();
    let key = kdf::derive(password, salt.clone(), params).await?;
    let sealed = aead::seal(&key, EXPORT_AAD, plaintext)?;
    Ok(SealedExport {
        kdf: KDF_NAME.to_owned(),
        params,
        salt,
        sealed,
    })
}

/// The content of `export` opened with `password`, by a process that seals its own exports with
/// `ours`: nothing below [`KdfParams::MIN`] is read, in a debug build only as weak as `ours` -
/// the tests' own parameters (RC-080-68).
///
/// # Errors
///
/// [`VaultError::TooCostly`] for parameters above [`KdfParams::IMPORT_MAX`], before anything is
/// derived; [`VaultError::BadParameters`] for another derivation, parameters below the floor or
/// a salt of the wrong length; [`VaultError::Malformed`] for a value not in the vault's format;
/// [`VaultError::WrongPassword`] when the tag fails - a wrong password or a changed byte.
pub async fn open_export(
    password: &MasterPassword,
    export: &SealedExport,
    ours: KdfParams,
) -> Result<SecretBytes> {
    if export.kdf != KDF_NAME || export.salt.len() != kdf::SALT_LEN {
        return Err(VaultError::BadParameters);
    }
    let params = export.params.checked_import(ours)?;
    let key = kdf::derive(password, export.salt.clone(), params).await?;
    match aead::open(&key, EXPORT_AAD, &export.sealed) {
        Ok(plaintext) => Ok(SecretBytes::from(plaintext.to_vec())),
        Err(VaultError::Malformed) => Err(VaultError::Malformed),
        Err(_) => Err(VaultError::WrongPassword),
    }
}

#[cfg(test)]
#[path = "envelope_tests.rs"]
mod tests;
