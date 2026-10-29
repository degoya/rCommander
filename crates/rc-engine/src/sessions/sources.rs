//! What a session asks the engine for: the host-key verdict and the secrets; and its failures
//! as engine errors.
//!
//! The gate is [`Engine::check_host_key`]: a key that is not trusted files the question for a
//! person and refuses. The credentials are [`Engine::resolve_secret`]'s, read one at a time when
//! their method's turn comes. For MCP (`announce` off) a locked vault is not announced, so no
//! unlock dialog opens for an agent's call: the answer is `vault_locked`, nothing more.

use async_trait::async_trait;
use rc_remote::{
    OfferedKey, SshError,
    ssh::{CredentialError, Credentials, HostKeyGate, HostKeyRefusal, KeyMaterial},
};
use rc_vault::SecretString;

use crate::{ConnectionId, Engine, EngineError, SecretKind, SecretRef};

/// The trust store, with its questions for a person.
pub(crate) struct EngineGate {
    pub engine: Engine,
}

#[async_trait]
impl HostKeyGate for EngineGate {
    async fn check(
        &self,
        host: &str,
        port: u16,
        offered: OfferedKey,
    ) -> Result<(), HostKeyRefusal> {
        match self.engine.check_host_key(host, port, offered).await {
            Ok(()) => Ok(()),
            Err(EngineError::HostKeyUnknown { .. }) => Err(HostKeyRefusal::Unknown),
            Err(EngineError::HostKeyChanged {
                stored_fingerprint, ..
            }) => Err(HostKeyRefusal::Changed { stored_fingerprint }),
            Err(error) => Err(HostKeyRefusal::Failed {
                reason: error.to_string(),
            }),
        }
    }

    async fn known_types(&self, host: &str, port: u16) -> Vec<String> {
        self.engine.stored_host_key_types(host, port).await
    }
}

/// One connection's secrets from the vault, or its key file.
pub(crate) struct EngineCredentials {
    pub engine: Engine,
    pub connection: ConnectionId,
    /// The key file to use when no key is stored in the vault.
    pub key_path: Option<String>,
    /// Whether a locked vault is announced (`SecretNeeded`) for the GUI to ask; off for MCP.
    pub announce: bool,
}

impl EngineCredentials {
    async fn secret(&self, kind: SecretKind) -> Result<Option<SecretString>, CredentialError> {
        self.engine
            .secret_for(
                SecretRef {
                    connection: self.connection,
                    kind,
                },
                self.announce,
            )
            .await
            .map_err(credential_error)
    }
}

#[async_trait]
impl Credentials for EngineCredentials {
    async fn password(&self) -> Result<Option<SecretString>, CredentialError> {
        self.secret(SecretKind::Password).await
    }

    async fn private_key(&self) -> Result<Option<KeyMaterial>, CredentialError> {
        let key = match self.secret(SecretKind::Key).await? {
            Some(key) => key,
            None => match self.key_path.as_deref() {
                // Refused unread unless local: opening a share would hand it the person's
                // credentials (RC-050-06).
                Some(path) => crate::commands::read_local_key_file(path)
                    .await
                    .map_err(credential_error)?,
                None => return Ok(None),
            },
        };
        let passphrase = self.secret(SecretKind::Passphrase).await?;
        Ok(Some(KeyMaterial { key, passphrase }))
    }
}

fn credential_error(error: EngineError) -> CredentialError {
    match error {
        EngineError::VaultLocked => CredentialError::Locked,
        other => CredentialError::Failed {
            code: other.code(),
            message: other.to_string(),
        },
    }
}

impl From<SshError> for EngineError {
    fn from(error: SshError) -> Self {
        match error {
            SshError::Cancelled => Self::Cancelled,
            SshError::Credential(CredentialError::Locked) => Self::VaultLocked,
            SshError::Key(error) => error.into(),
            SshError::HostKey {
                host,
                port,
                offered,
                refusal: HostKeyRefusal::Unknown,
            } => Self::HostKeyUnknown {
                host,
                port,
                algorithm: offered.algorithm,
                fingerprint: offered.fingerprint,
            },
            SshError::HostKey {
                host,
                port,
                offered,
                refusal: HostKeyRefusal::Changed { stored_fingerprint },
            } => Self::HostKeyChanged {
                host,
                port,
                algorithm: offered.algorithm,
                fingerprint: offered.fingerprint,
                stored_fingerprint,
            },
            other => Self::Ssh(other),
        }
    }
}

#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;
