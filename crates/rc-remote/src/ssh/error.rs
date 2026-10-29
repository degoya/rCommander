//! Why a session did not come up or went away, each with a stable code.
//!
//! The transport's own error text is not carried into a message: a server's disconnect reason
//! is free text from the other side. No variant holds a secret; a host key is public.

use crate::{
    HOST_KEY_CHANGED, HOST_KEY_UNKNOWN, KeyError, OfferedKey, PrivateKeyError,
    ssh::{AuthKind, Stage},
};

/// No user name is set for the connection.
pub const SSH_NO_USERNAME: &str = "ssh_no_username";
/// The server cannot be reached: name resolution, a refused or unreachable TCP connect.
pub const SSH_UNREACHABLE: &str = "ssh_unreachable";
/// A step of the setup took longer than its timeout.
pub const SSH_TIMEOUT: &str = "ssh_timeout";
/// The host-key check itself failed (the trust store could not be read).
pub const SSH_HOST_KEY_CHECK_FAILED: &str = "ssh_host_key_check_failed";
/// No method in the connection's order signed the login in.
pub const SSH_AUTH_FAILED: &str = "ssh_auth_failed";
/// The session went away: the server closed it, or keepalives stayed unanswered.
pub const CONNECTION_LOST: &str = "connection_lost";
/// The server broke the protocol or refused a request.
pub const SSH_PROTOCOL: &str = "ssh_protocol";
/// The setup was cancelled.
pub const CANCELLED: &str = "cancelled";
/// A secret was needed and the vault is locked.
pub const VAULT_LOCKED: &str = "vault_locked";

/// Why the host-key gate refused a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyRefusal {
    /// Nothing is stored for this host and key type: a person has to decide.
    Unknown,
    /// Another key of this type is stored: a hard stop.
    Changed { stored_fingerprint: String },
    /// The check could not be made.
    Failed { reason: String },
}

/// Why a secret could not be had for the login.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CredentialError {
    /// A secret is stored and the vault is locked; only a person can unlock it.
    #[error("a stored secret is needed and the vault is locked")]
    Locked,
    /// Reading it failed; `code` is the source's stable code.
    #[error("{message}")]
    Failed { code: &'static str, message: String },
}

/// Why an SSH session failed.
#[derive(Debug, thiserror::Error)]
pub enum SshError {
    #[error("the connection names no user to log in as")]
    NoUsername,
    #[error("cannot reach the SSH server {host}:{port}: {reason}")]
    Unreachable {
        host: String,
        port: u16,
        reason: String,
    },
    #[error("the SSH server did not complete the {stage} within {seconds} s")]
    Timeout { stage: Stage, seconds: u64 },
    #[error("cancelled")]
    Cancelled,
    #[error("the SSH server {host}:{port} presented a key ({}) that is not trusted", .offered.fingerprint)]
    HostKey {
        host: String,
        port: u16,
        offered: Box<OfferedKey>,
        refusal: HostKeyRefusal,
    },
    #[error(transparent)]
    HostKeyUnreadable(#[from] KeyError),
    #[error(transparent)]
    Credential(#[from] CredentialError),
    #[error(transparent)]
    Key(#[from] PrivateKeyError),
    #[error("the SSH server accepted none of the methods tried ({})", list(.tried))]
    AuthFailed { tried: Vec<AuthKind> },
    #[error("the SSH connection was lost")]
    ConnectionLost,
    #[error("the SSH connection failed: {reason}")]
    Protocol { reason: String },
}

impl SshError {
    /// Every code of the session's own failures, in declaration order; a credential source's
    /// failure keeps the code of its source, a host key that does not parse one of
    /// [`KeyError::ALL_CODES`].
    pub const ALL_CODES: &[&str] = &[
        SSH_NO_USERNAME,
        SSH_UNREACHABLE,
        SSH_TIMEOUT,
        CANCELLED,
        HOST_KEY_UNKNOWN,
        HOST_KEY_CHANGED,
        SSH_HOST_KEY_CHECK_FAILED,
        VAULT_LOCKED,
        "key_format_unknown",
        "key_passphrase_needed",
        "key_passphrase_wrong",
        SSH_AUTH_FAILED,
        CONNECTION_LOST,
        SSH_PROTOCOL,
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoUsername => SSH_NO_USERNAME,
            Self::Unreachable { .. } => SSH_UNREACHABLE,
            Self::Timeout { .. } => SSH_TIMEOUT,
            Self::Cancelled => CANCELLED,
            Self::HostKey { refusal, .. } => match refusal {
                HostKeyRefusal::Unknown => HOST_KEY_UNKNOWN,
                HostKeyRefusal::Changed { .. } => HOST_KEY_CHANGED,
                HostKeyRefusal::Failed { .. } => SSH_HOST_KEY_CHECK_FAILED,
            },
            Self::HostKeyUnreadable(error) => error.code(),
            Self::Credential(CredentialError::Locked) => VAULT_LOCKED,
            Self::Credential(CredentialError::Failed { code, .. }) => code,
            Self::Key(error) => match error {
                PrivateKeyError::Unknown => "key_format_unknown",
                PrivateKeyError::PassphraseNeeded => "key_passphrase_needed",
                PrivateKeyError::PassphraseWrong => "key_passphrase_wrong",
            },
            Self::AuthFailed { .. } => SSH_AUTH_FAILED,
            Self::ConnectionLost => CONNECTION_LOST,
            Self::Protocol { .. } => SSH_PROTOCOL,
        }
    }

    /// A transport failure of russh, without the other side's free text.
    pub(crate) fn transport(error: &russh::Error) -> Self {
        match error {
            russh::Error::Disconnect
            | russh::Error::HUP
            | russh::Error::SendError
            | russh::Error::KeepaliveTimeout
            | russh::Error::InactivityTimeout => Self::ConnectionLost,
            russh::Error::IO(io) => Self::Protocol {
                reason: io.kind().to_string(),
            },
            other => Self::Protocol {
                reason: protocol_reason(other),
            },
        }
    }
}

/// A short, fixed description of a protocol failure.
fn protocol_reason(error: &russh::Error) -> String {
    match error {
        russh::Error::NoCommonAlgo { kind, .. } => format!("no common {kind:?} algorithm"),
        russh::Error::Version => "not an SSH server".to_owned(),
        russh::Error::ChannelOpenFailure(reason) => format!("channel refused ({reason:?})"),
        russh::Error::RequestDenied => "the server refused the request".to_owned(),
        _ => "protocol error".to_owned(),
    }
}

fn list(tried: &[AuthKind]) -> String {
    if tried.is_empty() {
        return "no method had what it needs".to_owned();
    }
    tried
        .iter()
        .map(|kind| kind.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
