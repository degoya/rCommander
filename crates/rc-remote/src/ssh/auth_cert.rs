//! The login with the OpenSSH user certificate stored with the connection's key (RC-060-09).
//!
//! A certificate outside its validity is not sent: the server would refuse it, and the person
//! learns why instead of a bare refusal. One the server refuses leaves the key to try alone, as
//! OpenSSH's client does; the refusal names the principals when the user is not among them.
//!
//! The key signs through [`KeySigner`], not russh's own certificate method: that one names an
//! RSA certificate `ssh-rsa-cert-v01@openssh.com` (SHA-1, refused by OpenSSH since 8.8) and
//! signs it with SHA-512. Here the request names the hash the server takes, and the signature
//! uses it.

use std::{future::Future, sync::Arc};

use russh::{
    client::Handle,
    keys::{Algorithm, HashAlg, PrivateKey, agent::AgentIdentity, signature::Signer as _},
};

use super::rsa_hash;
use crate::{
    certificate::{self, CertificateError},
    ssh::{SshError, handler::ClientHandler},
};

/// How the certificate's turn ended.
pub(super) enum Signed {
    /// The server let the user in; the certificate's key id.
    In { key_id: String },
    /// Not sent, or refused: why, when this side can tell.
    Out(Option<CertificateError>),
}

/// Logs `user` in with the certificate `text` of `key`.
///
/// # Errors
///
/// A lost connection or a protocol failure.
pub(super) async fn with_stored(
    handle: &mut Handle<ClientHandler>,
    user: &str,
    key: &Arc<PrivateKey>,
    text: &str,
) -> Result<Signed, SshError> {
    let unusable = |error: CertificateError| -> Result<Signed, SshError> {
        tracing::debug!(code = error.code(), "the stored certificate is not sent");
        Ok(Signed::Out(Some(error)))
    };
    let parsed = match certificate::parse(text) {
        Ok(parsed) => parsed,
        Err(error) => return unusable(error),
    };
    if !certificate::certifies(&parsed, key.public_key()) {
        return unusable(CertificateError::KeyMismatch);
    }
    let info = certificate::describe(&parsed, text);
    if let Err(error) = certificate::check_validity(&info, crate::totp::unix_now()) {
        return unusable(error);
    }
    let hash = rsa_hash(handle, &parsed.algorithm()).await?;
    let mut signer = KeySigner {
        key: Arc::clone(key),
    };
    match handle
        .authenticate_certificate_with(user, parsed, hash, &mut signer)
        .await
    {
        Ok(result) if result.success() => {
            return Ok(Signed::In {
                key_id: info.key_id,
            });
        }
        Ok(_) => {}
        Err(SignError::Send(_)) => return Err(SshError::ConnectionLost),
        Err(SignError::Sign) => tracing::debug!("the key did not sign the certificate's login"),
    }
    Ok(Signed::Out(refused_for(&info, user)))
}

/// What a refusal of `info` for `user` can name: the principals, when they leave the user out.
pub(super) fn refused_for(
    info: &certificate::KeyCertificate,
    user: &str,
) -> Option<CertificateError> {
    (!info.names(user)).then(|| CertificateError::Principal {
        key_id: info.key_id.clone(),
        user: user.to_owned(),
        principals: info.principals.clone(),
    })
}

/// Why [`KeySigner`] did not sign.
#[derive(Debug, thiserror::Error)]
pub(super) enum SignError {
    #[error(transparent)]
    Send(#[from] russh::SendError),
    #[error("the key did not sign")]
    Sign,
}

/// Signs a certificate's login with the private key in memory, with the hash of the request.
pub(super) struct KeySigner {
    key: Arc<PrivateKey>,
}

impl russh::Signer for KeySigner {
    type Error = SignError;

    fn auth_sign(
        &mut self,
        _key: &AgentIdentity,
        hash_alg: Option<HashAlg>,
        mut to_sign: Vec<u8>,
    ) -> impl Future<Output = Result<Vec<u8>, Self::Error>> + Send {
        let signed = sign(&self.key, hash_alg, &to_sign).map(|signature| {
            // The signature follows the signed data as one SSH string, as an agent answers.
            let length = u32::try_from(signature.len()).unwrap_or(u32::MAX);
            to_sign.extend_from_slice(&length.to_be_bytes());
            to_sign.extend_from_slice(&signature);
            to_sign
        });
        std::future::ready(signed)
    }
}

/// `data` signed by `key` - an RSA key with `hash` - as the SSH signature blob.
fn sign(key: &PrivateKey, hash: Option<HashAlg>, data: &[u8]) -> Result<Vec<u8>, SignError> {
    use russh::keys::ssh_encoding::Encode as _;

    let signature = match (key.algorithm(), key.key_data().rsa()) {
        (Algorithm::Rsa { .. }, Some(rsa)) => (rsa, hash).try_sign(data),
        _ => key.try_sign(data),
    }
    .map_err(|_| SignError::Sign)?;
    signature.encode_vec().map_err(|_| SignError::Sign)
}

#[cfg(test)]
#[path = "auth_cert_tests.rs"]
mod tests;
