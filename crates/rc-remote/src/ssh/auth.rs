//! The login: the methods of the connection's order, one after the other, until the server
//! accepts one.
//!
//! A method without its material is skipped without asking anyone: no agent running, no key,
//! no stored password. Secrets are asked for only when their method's turn comes, so a login an
//! agent signs never touches the vault. A key is opened off the async threads and zeroizes
//! itself on drop; a password answers the server's `password` method, or its single hidden
//! `keyboard-interactive` prompt when the server offers only that. A prompt a person would have
//! to answer (several, or an echoed one) is not guessed at.

use std::sync::Arc;

use russh::{
    MethodKind,
    client::{AuthResult, Handle, KeyboardInteractiveAuthResponse},
    keys::{Algorithm, HashAlg, PrivateKeyWithHashAlg, PublicKey, agent::AgentIdentity},
};
use secrecy::{ExposeSecret as _, SecretString};
use tokio::time::timeout;

use super::agent_offer::offer_order;
use crate::{
    PrivateKeyError,
    agent::{self, AGENT_WAIT},
    private_key,
    ssh::{AuthKind, AuthUsed, Credentials, SessionOptions, SshError, handler::ClientHandler},
};

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
    Accepted(AuthUsed),
}

/// Runs the methods of `options.auth_order` until one is accepted.
///
/// # Errors
///
/// [`SshError::AuthFailed`] naming the methods tried, [`SshError::Key`] when the key does not
/// open and nothing else got in, a credential source's failure (a locked vault) at once, and a
/// lost connection.
pub(super) async fn authenticate(
    handle: &mut Handle<ClientHandler>,
    user: &str,
    options: &SessionOptions,
    credentials: &dyn Credentials,
) -> Result<AuthUsed, SshError> {
    let mut tried = Vec::new();
    let mut key_failure = None;
    for kind in &options.auth_order {
        let attempt = match kind {
            AuthKind::Agent => by_agent(handle, user, options).await?,
            AuthKind::Key => by_key(handle, user, credentials).await?,
            AuthKind::Password => by_password(handle, user, credentials).await?,
        };
        match attempt {
            Attempt::Skipped => {}
            Attempt::Refused => tried.push(*kind),
            Attempt::KeyUnusable(error) => {
                key_failure.get_or_insert(error);
            }
            Attempt::Accepted(used) => return Ok(used),
        }
        if handle.is_closed() {
            break;
        }
    }
    Err(key_failure.map_or(SshError::AuthFailed { tried }, SshError::Key))
}

/// The keys of every agent: the connection's own key first, then the others in the agents'
/// order, at most [`SessionOptions::max_agent_keys`] of them; only the chosen one when a
/// fingerprint is set. An agent that does not answer is passed over.
async fn by_agent(
    handle: &mut Handle<ClientHandler>,
    user: &str,
    options: &SessionOptions,
) -> Result<Attempt, SshError> {
    let mut agents = Vec::new();
    for source in &options.agents {
        let mut agent = match agent::connect(source).await {
            Ok(agent) => agent,
            Err(error) => {
                tracing::debug!(%error, "SSH agent not used");
                continue;
            }
        };
        let identities = match timeout(AGENT_WAIT, agent.request_identities()).await {
            Ok(Ok(identities)) => identities,
            Ok(Err(error)) => {
                tracing::debug!(%error, agent = %source.kind(), "SSH agent did not list its keys");
                continue;
            }
            Err(_) => {
                tracing::debug!(agent = %source.kind(), "SSH agent did not answer in time");
                continue;
            }
        };
        // A certificate needs another flow; a plain key further down may still do.
        let keys: Vec<PublicKey> = identities
            .into_iter()
            .filter_map(|identity| match identity {
                AgentIdentity::PublicKey { key, .. } => Some(key),
                _ => None,
            })
            .collect();
        agents.push((source.kind(), agent, keys));
    }
    let listed: Vec<Vec<String>> = agents
        .iter()
        .map(|(_, _, keys)| {
            keys.iter()
                .map(|key| key.fingerprint(HashAlg::Sha256).to_string())
                .collect()
        })
        .collect();
    let order = offer_order(
        &listed,
        options.agent_fingerprint.as_deref(),
        options.own_key.as_deref(),
        options.max_agent_keys,
    );
    for &(index, key_index) in &order {
        let (Some((kind, agent, keys)), Some(fingerprints)) =
            (agents.get_mut(index), listed.get(index))
        else {
            continue;
        };
        let (Some(key), Some(fingerprint)) = (keys.get(key_index), fingerprints.get(key_index))
        else {
            continue;
        };
        let hash = rsa_hash(handle, key).await?;
        match handle
            .authenticate_publickey_with(user, key.clone(), hash, agent)
            .await
        {
            Ok(result) if result.success() => {
                return Ok(Attempt::Accepted(AuthUsed::Agent {
                    source: *kind,
                    fingerprint: fingerprint.clone(),
                }));
            }
            Ok(_) => {}
            Err(error) => tracing::debug!(%error, "the SSH agent did not sign"),
        }
        if handle.is_closed() {
            return Err(SshError::ConnectionLost);
        }
    }
    Ok(if order.is_empty() {
        Attempt::Skipped
    } else {
        Attempt::Refused
    })
}

/// The connection's private key with its passphrase.
async fn by_key(
    handle: &mut Handle<ClientHandler>,
    user: &str,
    credentials: &dyn Credentials,
) -> Result<Attempt, SshError> {
    let Some(material) = credentials.private_key().await? else {
        return Ok(Attempt::Skipped);
    };
    let opened = tokio::task::spawn_blocking(move || {
        private_key::decode(
            material.key.expose_secret(),
            material
                .passphrase
                .as_ref()
                .map(|passphrase| passphrase.expose_secret()),
        )
    })
    .await;
    let key = match opened {
        Ok(Ok(key)) => key,
        Ok(Err(error)) => return Ok(Attempt::KeyUnusable(error)),
        Err(_) => return Ok(Attempt::KeyUnusable(PrivateKeyError::Unknown)),
    };
    let fingerprint = key.public_key().fingerprint(HashAlg::Sha256).to_string();
    let hash = rsa_hash(handle, key.public_key()).await?;
    let result = handle
        .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
        .await
        .map_err(|error| SshError::transport(&error))?;
    Ok(if result.success() {
        Attempt::Accepted(AuthUsed::Key { fingerprint })
    } else {
        Attempt::Refused
    })
}

/// The stored password, over `password` or the server's `keyboard-interactive` prompt.
async fn by_password(
    handle: &mut Handle<ClientHandler>,
    user: &str,
    credentials: &dyn Credentials,
) -> Result<Attempt, SshError> {
    let Some(password) = credentials.password().await? else {
        return Ok(Attempt::Skipped);
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
            keyboard_interactive(handle, user, &password).await
        }
        AuthResult::Failure { .. } => Ok(Attempt::Refused),
    }
}

/// Answers the server's one hidden prompt with the password; empty rounds are answered empty.
async fn keyboard_interactive(
    handle: &mut Handle<ClientHandler>,
    user: &str,
    password: &SecretString,
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
        let answers = match prompts.as_slice() {
            [] => Vec::new(),
            // russh takes the answers as `Vec<String>` and keeps them: the copy cannot be
            // zeroized (RC-080-51), as no password handed to russh can.
            [prompt] if !prompt.echo => vec![password.expose_secret().to_owned()],
            _ => return Ok(Attempt::Refused),
        };
        reply = handle
            .authenticate_keyboard_interactive_respond(answers)
            .await
            .map_err(|error| SshError::transport(&error))?;
    }
    Ok(Attempt::Refused)
}

/// The signature hash for an RSA key: the best the server names, SHA-256 when it names none;
/// `None` for every other key type.
async fn rsa_hash(
    handle: &Handle<ClientHandler>,
    key: &PublicKey,
) -> Result<Option<HashAlg>, SshError> {
    if !matches!(key.algorithm(), Algorithm::Rsa { .. }) {
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
