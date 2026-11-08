//! The login: the methods of the connection's order, one after the other, until the server
//! accepts one.
//!
//! A method without its material is skipped without asking anyone: no agent running, no key,
//! no stored password. Secrets are asked for only when their method's turn comes, so a login an
//! agent signs never touches the vault. A key is opened off the async threads and zeroizes
//! itself on drop; a password answers the server's `password` method, or its hidden
//! `keyboard-interactive` prompt when the server offers only that. A question the connection's
//! code pattern matches gets the one-time code of its stored seed (RC-060-08, `auth_prompts.rs`),
//! made at the moment it is asked; a prompt a person would have to answer (a second hidden one,
//! an echoed one, a code question without a seed) is not guessed at. A key with an OpenSSH user
//! certificate - stored with it, or held by an agent - offers the certificate first (RC-060-09,
//! `auth_cert.rs`, `auth_agent.rs`).

use std::sync::Arc;

use russh::{
    MethodKind,
    client::{AuthResult, Handle, KeyboardInteractiveAuthResponse},
    keys::{Algorithm, HashAlg, PrivateKeyWithHashAlg},
};
use secrecy::{ExposeSecret as _, SecretString};

use self::agents::by_agent;
use self::cert::Signed;
use super::auth_prompts::{Answer, answers};
use crate::recover::LogPanic as _;
use crate::{
    CertificateError, PrivateKeyError, private_key,
    ssh::{AuthKind, AuthUsed, Credentials, SessionOptions, SshError, handler::ClientHandler},
};

#[path = "auth_agent.rs"]
mod agents;
#[path = "auth_cert.rs"]
mod cert;

/// The rounds of `keyboard-interactive` questions answered at most.
const MAX_PROMPT_ROUNDS: usize = 4;

/// One method's turn.
enum Attempt {
    /// Nothing to try with.
    Skipped,
    /// Tried and refused.
    Refused,
    /// The key could not be opened; the next method still gets its turn.
    KeyUnusable(PrivateKeyError),
    /// Refused, or its certificate not sent, for a reason the certificate tells (RC-060-09).
    Certificate(CertificateError),
    Accepted(AuthUsed),
}

/// Runs the methods of `options.auth_order` until one is accepted.
///
/// # Errors
///
/// [`SshError::AuthFailed`] naming the methods tried, [`SshError::Key`] when the key does not
/// open and nothing else got in, [`SshError::Certificate`] when a certificate was not sent or
/// refused for a reason it tells and nothing else got in, a credential source's failure (a
/// locked vault) at once, and a lost connection.
pub(super) async fn authenticate(
    handle: &mut Handle<ClientHandler>,
    user: &str,
    options: &SessionOptions,
    credentials: &dyn Credentials,
) -> Result<AuthUsed, SshError> {
    let mut tried = Vec::new();
    let mut key_failure = None;
    let mut certificate_failure = None;
    for kind in &options.auth_order {
        let attempt = match kind {
            AuthKind::Agent => by_agent(handle, user, options).await?,
            AuthKind::Key => by_key(handle, user, options, credentials).await?,
            AuthKind::Password => by_password(handle, user, options, credentials).await?,
        };
        match attempt {
            Attempt::Skipped => {}
            Attempt::Refused => tried.push(*kind),
            Attempt::KeyUnusable(error) => {
                key_failure.get_or_insert(error);
            }
            Attempt::Certificate(error) => {
                tried.push(*kind);
                certificate_failure.get_or_insert(error);
            }
            Attempt::Accepted(used) => return Ok(used),
        }
        if handle.is_closed() {
            break;
        }
    }
    Err(match (key_failure, certificate_failure) {
        (Some(error), _) => SshError::Key(error),
        (None, Some(error)) => SshError::Certificate(error),
        (None, None) => SshError::AuthFailed { tried },
    })
}

/// The connection's private key with its passphrase; with its certificate first when one is
/// stored and certificates are used (RC-060-09). A certificate not sent or refused leaves the key
/// to try alone; when that is refused too, the certificate's reason is the answer.
async fn by_key(
    handle: &mut Handle<ClientHandler>,
    user: &str,
    options: &SessionOptions,
    credentials: &dyn Credentials,
) -> Result<Attempt, SshError> {
    let Some(mut material) = credentials.private_key().await? else {
        return Ok(Attempt::Skipped);
    };
    let certificate = material
        .certificate
        .take()
        .filter(|_| options.use_certificates);
    let opened = tokio::task::spawn_blocking(move || {
        private_key::decode(
            material.key.expose_secret(),
            material
                .passphrase
                .as_ref()
                .map(|passphrase| passphrase.expose_secret()),
        )
    })
    .await
    .log_panic("remote.private_key_decode");
    let key = match opened {
        Ok(Ok(key)) => Arc::new(key),
        Ok(Err(error)) => return Ok(Attempt::KeyUnusable(error)),
        Err(_) => return Ok(Attempt::KeyUnusable(PrivateKeyError::Unknown)),
    };
    let fingerprint = key.public_key().fingerprint(HashAlg::Sha256).to_string();
    let mut certificate_failure = None;
    if let Some(text) = certificate {
        match cert::with_stored(handle, user, &key, &text).await? {
            Signed::In { key_id } => {
                return Ok(Attempt::Accepted(AuthUsed::Key {
                    fingerprint,
                    certificate: Some(key_id),
                }));
            }
            Signed::Out(failure) => certificate_failure = failure,
        }
        if handle.is_closed() {
            return Err(SshError::ConnectionLost);
        }
    }
    let hash = rsa_hash(handle, &key.algorithm()).await?;
    let result = handle
        .authenticate_publickey(user, PrivateKeyWithHashAlg::new(key, hash))
        .await
        .map_err(|error| SshError::transport(&error))?;
    Ok(if result.success() {
        Attempt::Accepted(AuthUsed::Key {
            fingerprint,
            certificate: None,
        })
    } else {
        certificate_failure.map_or(Attempt::Refused, Attempt::Certificate)
    })
}

/// The stored password, over `password` or the server's `keyboard-interactive` prompt; without
/// one, a stored seed still answers a server that asks only for a code over
/// `keyboard-interactive` (after a key it took as the first factor, say).
async fn by_password(
    handle: &mut Handle<ClientHandler>,
    user: &str,
    options: &SessionOptions,
    credentials: &dyn Credentials,
) -> Result<Attempt, SshError> {
    let Some(password) = credentials.password().await? else {
        if credentials.totp_code().await?.is_none() {
            return Ok(Attempt::Skipped);
        }
        return keyboard_interactive(handle, user, None, options, credentials).await;
    };
    let result = handle
        .authenticate_password(user, password.expose_secret())
        .await
        .map_err(|error| SshError::transport(&error))?;
    match result {
        AuthResult::Success => Ok(Attempt::Accepted(AuthUsed::Password)),
        // Only when the server does not take passwords that way; a wrong password is not sent
        // twice.
        AuthResult::Failure {
            remaining_methods, ..
        } if remaining_methods.contains(&MethodKind::KeyboardInteractive)
            && !remaining_methods.contains(&MethodKind::Password) =>
        {
            keyboard_interactive(handle, user, Some(&password), options, credentials).await
        }
        AuthResult::Failure { .. } => Ok(Attempt::Refused),
    }
}

/// Answers the server's rounds of questions: the password to its hidden one, the one-time code
/// to those the code pattern matches (`auth_prompts.rs`); empty rounds are answered empty. A round
/// with a question for a person, or a code question without a code, refuses the method.
async fn keyboard_interactive(
    handle: &mut Handle<ClientHandler>,
    user: &str,
    password: Option<&SecretString>,
    options: &SessionOptions,
    credentials: &dyn Credentials,
) -> Result<Attempt, SshError> {
    let mut reply = handle
        .authenticate_keyboard_interactive_start(user, None::<String>)
        .await
        .map_err(|error| SshError::transport(&error))?;
    for _ in 0..MAX_PROMPT_ROUNDS {
        let prompts = match reply {
            KeyboardInteractiveAuthResponse::Success => {
                return Ok(Attempt::Accepted(AuthUsed::KeyboardInteractive));
            }
            KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(Attempt::Refused),
            KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => prompts,
        };
        let questions = prompts
            .iter()
            .map(|prompt| (prompt.prompt.as_str(), prompt.echo));
        let Some(plan) = answers(questions, &options.totp_prompt, password.is_some()) else {
            return Ok(Attempt::Refused);
        };
        // Made now, not at the start of the login: the code is the one of the moment it is asked.
        let code = if plan.contains(&Answer::Code) {
            match credentials.totp_code().await? {
                Some(code) => Some(code),
                // No seed, or filling codes in is switched off: the code is the person's.
                None => return Ok(Attempt::Refused),
            }
        } else {
            None
        };
        // russh takes the answers as `Vec<String>` and keeps them: the copy cannot be
        // zeroized (RC-080-51), as no password handed to russh can.
        let secret = |value: Option<&SecretString>| {
            value.map_or_else(String::new, |value| value.expose_secret().to_owned())
        };
        let replies = plan
            .iter()
            .map(|answer| match answer {
                Answer::Password => secret(password),
                Answer::Code => secret(code.as_ref()),
            })
            .collect();
        if code.is_some() {
            tracing::debug!("a one-time code answers the server's question");
        }
        reply = handle
            .authenticate_keyboard_interactive_respond(replies)
            .await
            .map_err(|error| SshError::transport(&error))?;
    }
    Ok(Attempt::Refused)
}

/// The signature hash for an RSA key (or certificate) of `algorithm`: the best the server
/// names, SHA-256 when it names none; `None` for every other key type.
async fn rsa_hash(
    handle: &Handle<ClientHandler>,
    algorithm: &Algorithm,
) -> Result<Option<HashAlg>, SshError> {
    if !matches!(algorithm, Algorithm::Rsa { .. }) {
        return Ok(None);
    }
    match handle.best_supported_rsa_hash().await {
        Ok(Some(hash)) => Ok(hash),
        Ok(None) => Ok(Some(HashAlg::Sha256)),
        Err(error) => Err(SshError::transport(&error)),
    }
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "auth_totp_tests.rs"]
mod totp_tests;
