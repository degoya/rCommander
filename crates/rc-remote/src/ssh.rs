//! SSH sessions on russh (RC-060-01): connect, check the host key, log in, open channels.
//!
//! [`SshSession::connect`] takes the target, the [`SessionOptions`] (timeouts, keepalive, the
//! order of the methods, the agents), a [`HostKeyGate`] and a [`Credentials`] source. Neither
//! decision lives here: the gate is the trust store's verdict (the engine files the question for
//! a person, RC-050-07), and the credentials are the vault's (RC-050-03), asked for one at a time
//! when their method's turn comes. Nothing here stores, logs or prints a secret; the log names
//! the method that got in and, for a key, its fingerprint.
//!
//! The session is one TCP connection that carries every channel (SFTP, terminal, command);
//! [`open_pty_shell`] turns a channel into the byte stream a terminal or a login script drives,
//! [`exec_command`] runs one command on a channel and collects its output (RC-060-04).

mod agent_offer;
mod auth;
mod config;
mod error;
mod exec;
mod handler;
mod session;

use async_trait::async_trait;
use secrecy::SecretString;

pub use config::{
    AuthKind, AuthUsed, DEFAULT_AUTH_TIMEOUT, DEFAULT_CONNECT_TIMEOUT, DEFAULT_HANDSHAKE_TIMEOUT,
    DEFAULT_KEEPALIVE, DEFAULT_MAX_AGENT_KEYS, KEEPALIVE_MAX, MAX_AGENT_KEYS, SessionOptions,
    SshTarget, Stage, format_order, parse_order,
};
pub use error::{
    CONNECTION_LOST, CredentialError, HostKeyRefusal, SSH_AUTH_FAILED, SSH_HOST_KEY_CHECK_FAILED,
    SSH_NO_USERNAME, SSH_PROTOCOL, SSH_TIMEOUT, SSH_UNREACHABLE, SshError,
};
pub use exec::{
    DEFAULT_EXEC_OUTPUT, DEFAULT_EXEC_TIMEOUT, ExecLimits, ExecOutput, MAX_EXEC_OUTPUT,
    MAX_EXEC_TIMEOUT, exec_command,
};
pub use session::{SshSession, open_pty_shell, start_pty_shell};

use crate::OfferedKey;

/// A session channel: SFTP, a terminal or a command runs on one.
pub type SshChannel = russh::Channel<russh::client::Msg>;

/// A channel with a terminal and a shell as a byte stream (`AsyncRead` + `AsyncWrite`).
pub type PtyStream = russh::ChannelStream<russh::client::Msg>;

/// The decision about the key a server presented, made outside the session.
#[async_trait]
pub trait HostKeyGate: Send + Sync {
    /// `Ok` lets the session go on with `offered` for `host` on `port`.
    ///
    /// # Errors
    ///
    /// Why the key is refused; the session fails with [`SshError::HostKey`].
    async fn check(&self, host: &str, port: u16, offered: OfferedKey)
    -> Result<(), HostKeyRefusal>;

    /// The key types trusted for `host` on `port`, which the key exchange asks the server for
    /// first (RC-080-12); none by default.
    async fn known_types(&self, _host: &str, _port: u16) -> Vec<String> {
        Vec::new()
    }
}

/// The secrets of one connection, asked for one at a time and only when needed.
#[async_trait]
pub trait Credentials: Send + Sync {
    /// The stored password; `None` when none is stored.
    ///
    /// # Errors
    ///
    /// [`CredentialError::Locked`] when one is stored and the vault is locked; a read failure.
    async fn password(&self) -> Result<Option<SecretString>, CredentialError>;

    /// The private key and its passphrase; `None` when there is no key.
    ///
    /// # Errors
    ///
    /// As [`Credentials::password`].
    async fn private_key(&self) -> Result<Option<KeyMaterial>, CredentialError>;
}

/// A private key's text and its passphrase, as long as the login needs them.
#[derive(Debug)]
pub struct KeyMaterial {
    pub key: SecretString,
    pub passphrase: Option<SecretString>,
}
