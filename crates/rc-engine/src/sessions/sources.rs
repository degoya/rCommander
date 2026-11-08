//! What a session asks the engine for: the host-key verdict and the secrets; and its failures
//! as engine errors.
//!
//! The gate is [`Engine::check_host_key`]: a key that is not trusted files the question for a
//! person and refuses. The credentials are [`Engine::resolve_secret`]'s, read one at a time when
//! their method's turn comes; the one-time code is made from the connection's seed when the
//! server asks for it, unless `connections.totp_autofill` is off (RC-060-08). For MCP (`announce` off) a locked vault is not announced, so no
//! unlock dialog opens for an agent's call: the answer is `vault_locked`, nothing more. A key
//! comes with its OpenSSH user certificate (RC-060-09) while `ssh.certificates` is on: the one
//! stored with it, or the `<key>-cert.pub` beside a key file.

use async_trait::async_trait;
use rc_remote::{
    OfferedKey, SshError,
    ssh::{CredentialError, Credentials, HostKeyGate, HostKeyRefusal, KeyMaterial},
};
use rc_vault::SecretString;

use crate::{
    ConnectionId, Engine, EngineError, HopLabel, SecretKind, SecretRef,
    settings::{SSH_CERTIFICATES_KEY, TOTP_AUTOFILL_KEY},
};

/// The trust store, with its questions for a person.
pub(crate) struct EngineGate {
    pub engine: Engine,
    /// Which hop of a chain of jump hosts the key is checked for (RC-110-03); the question says
    /// so. `None` for a direct session.
    pub hop: Option<HopLabel>,
}

#[async_trait]
impl HostKeyGate for EngineGate {
    async fn check(
        &self,
        host: &str,
        port: u16,
        offered: OfferedKey,
    ) -> Result<(), HostKeyRefusal> {
        let hop = self.hop.clone();
        match self
            .engine
            .check_host_key_at(host, port, offered, hop)
            .await
        {
            Ok(()) => Ok(()),
            Err(EngineError::HostKeyUnknown { .. }) => Err(HostKeyRefusal::Unknown),
            Err(EngineError::HostKeyChanged {
                stored_fingerprint, ..
            }) => Err(HostKeyRefusal::Changed { stored_fingerprint }),
            Err(EngineError::HostKeyRevoked { .. }) => Err(HostKeyRefusal::Revoked),
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
        let certificates = self.engine.setting_on(SSH_CERTIFICATES_KEY).await;
        let (key, certificate) = match self.secret(SecretKind::Key).await? {
            Some(key) => {
                let certificate = if certificates {
                    self.engine
                        .stored_certificate(self.connection)
                        .await
                        .map_err(credential_error)?
                } else {
                    None
                };
                (key, certificate)
            }
            None => match self.key_path.as_deref() {
                // Refused unread unless local: opening a share would hand it the person's
                // credentials (RC-050-06).
                Some(path) => {
                    let key = crate::commands::read_local_key_file(path)
                        .await
                        .map_err(credential_error)?;
                    let certificate = if certificates {
                        beside_certificate(path).await
                    } else {
                        None
                    };
                    (key, certificate)
                }
                None => return Ok(None),
            },
        };
        let passphrase = self.secret(SecretKind::Passphrase).await?;
        Ok(Some(KeyMaterial {
            key,
            passphrase,
            certificate,
        }))
    }

    async fn totp_code(&self) -> Result<Option<SecretString>, CredentialError> {
        if !self.engine.setting_on(TOTP_AUTOFILL_KEY).await {
            return Ok(None);
        }
        self.engine
            .totp_code(self.connection, self.announce)
            .await
            .map_err(credential_error)
    }
}

/// The `<path>-cert.pub` beside the key file `path`, for its login; one that cannot be read is
/// left out - the key still logs in alone - and logged.
pub(crate) async fn beside_certificate(path: &str) -> Option<String> {
    match crate::commands::read_beside_certificate(path).await {
        Ok(certificate) => certificate,
        Err(error) => {
            tracing::warn!(
                code = error.code(),
                "the certificate beside the key file is not used"
            );
            None
        }
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
            SshError::HostKey {
                host,
                port,
                offered,
                refusal: HostKeyRefusal::Revoked,
            } => Self::HostKeyRevoked {
                host,
                port,
                algorithm: offered.algorithm,
                fingerprint: offered.fingerprint,
            },
            other => Self::Ssh(other),
        }
    }
}

#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;
