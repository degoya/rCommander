//! OpenSSH user certificates (RC-060-09): a public key a CA signed, with the names it is valid
//! for (principals), a validity period and a key id - `id_ed25519-cert.pub` beside its key. A
//! server that trusts the CA (`TrustedUserCAKeys`) takes the certificate in place of the key in
//! `authorized_keys`.
//!
//! A certificate is public, like the key's public half, and is kept in clear beside the key
//! ([`rc_db::KeyCertificate`]). Its text is untrusted input: one line of at most
//! [`MAX_CERTIFICATE_BYTES`], read by `ssh-key`'s strict reader, a user certificate (never a host
//! one) that certifies the private key it is stored with. Whether the CA is trusted and the
//! principal fits is the server's decision; what this side can tell for certain - a certificate
//! outside its validity - is said before it is sent.

use russh::keys::{Certificate, HashAlg, PublicKey};

pub use rc_db::{CertificateValidity, KeyCertificate};

/// The largest certificate taken in, in bytes; an RSA-4096 certificate signed by an RSA-4096 CA
/// with a few principals is under 3 KiB.
pub const MAX_CERTIFICATE_BYTES: usize = 16 * 1024;

/// What the `-cert.pub` file beside a key file is called: `<key file>-cert.pub`, as OpenSSH
/// looks for it.
pub const CERTIFICATE_SUFFIX: &str = "-cert.pub";

/// The text is no OpenSSH user certificate.
pub const SSH_CERTIFICATE_INVALID: &str = "ssh_certificate_invalid";
/// The certificate certifies another key than the one it is used with.
pub const SSH_CERTIFICATE_KEY_MISMATCH: &str = "ssh_certificate_key_mismatch";
/// The certificate's validity ended.
pub const SSH_CERTIFICATE_EXPIRED: &str = "ssh_certificate_expired";
/// The certificate's validity has not begun.
pub const SSH_CERTIFICATE_NOT_YET_VALID: &str = "ssh_certificate_not_yet_valid";
/// The server refused the certificate, and it does not name the user among its principals.
pub const SSH_CERTIFICATE_PRINCIPAL: &str = "ssh_certificate_principal";

/// Why a certificate cannot be taken in, or was not used for a login.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CertificateError {
    /// Not one OpenSSH user certificate, damaged, a host certificate, or larger than
    /// [`MAX_CERTIFICATE_BYTES`].
    #[error("not an OpenSSH user certificate")]
    Invalid,
    /// It certifies another key.
    #[error("the certificate does not certify this private key")]
    KeyMismatch,
    /// Its validity ended at `valid_before` (Unix time); it was not sent.
    #[error("the certificate {key_id:?} expired (valid before {valid_before}, Unix time)")]
    Expired { key_id: String, valid_before: u64 },
    /// Its validity begins at `valid_after` (Unix time); it was not sent.
    #[error("the certificate {key_id:?} is valid only from {valid_after} (Unix time) on")]
    NotYetValid { key_id: String, valid_after: u64 },
    /// The server refused it, and its principals do not name `user`.
    #[error(
        "the SSH server refused the certificate {key_id:?}: it is valid for {}, not for {user:?}",
        .principals.join(", ")
    )]
    Principal {
        key_id: String,
        user: String,
        principals: Vec<String>,
    },
}

impl CertificateError {
    /// Every code, in declaration order.
    pub const ALL_CODES: &[&str] = &[
        SSH_CERTIFICATE_INVALID,
        SSH_CERTIFICATE_KEY_MISMATCH,
        SSH_CERTIFICATE_EXPIRED,
        SSH_CERTIFICATE_NOT_YET_VALID,
        SSH_CERTIFICATE_PRINCIPAL,
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Invalid => SSH_CERTIFICATE_INVALID,
            Self::KeyMismatch => SSH_CERTIFICATE_KEY_MISMATCH,
            Self::Expired { .. } => SSH_CERTIFICATE_EXPIRED,
            Self::NotYetValid { .. } => SSH_CERTIFICATE_NOT_YET_VALID,
            Self::Principal { .. } => SSH_CERTIFICATE_PRINCIPAL,
        }
    }
}

/// The certificate of the text `text`: one line, an OpenSSH user certificate.
///
/// # Errors
///
/// [`CertificateError::Invalid`] for anything else: a public key, a host certificate, a damaged
/// line, more than one line, more than [`MAX_CERTIFICATE_BYTES`].
pub fn parse(text: &str) -> Result<Certificate, CertificateError> {
    let text = text.trim();
    if text.is_empty() || text.len() > MAX_CERTIFICATE_BYTES || text.lines().count() != 1 {
        return Err(CertificateError::Invalid);
    }
    let certificate = Certificate::from_openssh(text).map_err(|_| CertificateError::Invalid)?;
    if !certificate.cert_type().is_user() {
        return Err(CertificateError::Invalid);
    }
    Ok(certificate)
}

/// What `certificate` says, for the window and MCP, with `text` as its line.
#[must_use]
pub fn describe(certificate: &Certificate, text: &str) -> KeyCertificate {
    let valid_before = certificate.valid_before();
    KeyCertificate {
        cert_type: certificate.algorithm().to_certificate_type(),
        key_id: certificate.key_id().to_owned(),
        serial: certificate.serial(),
        principals: certificate.valid_principals().to_vec(),
        valid_after: certificate.valid_after(),
        // OpenSSH writes "forever" as the largest value.
        valid_before: (valid_before != u64::MAX).then_some(valid_before),
        ca_fingerprint: certificate
            .signature_key()
            .fingerprint(HashAlg::Sha256)
            .to_string(),
        openssh: text.trim().to_owned(),
    }
}

/// The certificate `text` for the private key whose public half is `key`, described.
///
/// # Errors
///
/// [`CertificateError::Invalid`] as for [`parse`], [`CertificateError::KeyMismatch`] when it
/// certifies another key. A certificate outside its validity is taken: the window shows it so.
pub fn inspect(text: &str, key: &PublicKey) -> Result<KeyCertificate, CertificateError> {
    let certificate = parse(text)?;
    if !certifies(&certificate, key) {
        return Err(CertificateError::KeyMismatch);
    }
    Ok(describe(&certificate, text))
}

/// As [`inspect`], for the key whose public half is the `authorized_keys` line `public_key` (a
/// stored key's, [`rc_db::StoredKey::public_key`]).
///
/// # Errors
///
/// As [`inspect`]; a line that does not read certifies nothing ([`CertificateError::KeyMismatch`]).
pub fn inspect_line(text: &str, public_key: &str) -> Result<KeyCertificate, CertificateError> {
    let key =
        PublicKey::from_openssh(public_key.trim()).map_err(|_| CertificateError::KeyMismatch)?;
    inspect(text, &key)
}

/// Whether `certificate` certifies `key`.
#[must_use]
pub fn certifies(certificate: &Certificate, key: &PublicKey) -> bool {
    certificate.public_key() == key.key_data()
}

/// `Ok` when `info` is valid at the Unix time `now`, else why it is not sent.
///
/// # Errors
///
/// [`CertificateError::Expired`] or [`CertificateError::NotYetValid`].
pub fn check_validity(info: &KeyCertificate, now: u64) -> Result<(), CertificateError> {
    match info.validity_at(now) {
        CertificateValidity::Valid => Ok(()),
        CertificateValidity::Expired => Err(CertificateError::Expired {
            key_id: info.key_id.clone(),
            valid_before: info.valid_before.unwrap_or(u64::MAX),
        }),
        CertificateValidity::NotYetValid => Err(CertificateError::NotYetValid {
            key_id: info.key_id.clone(),
            valid_after: info.valid_after,
        }),
    }
}

#[cfg(test)]
#[path = "certificate_tests.rs"]
mod tests;
