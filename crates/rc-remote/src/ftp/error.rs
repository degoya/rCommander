//! What goes wrong with an FTP server, in two layers.
//!
//! [`FtpError`] is the failure to get a working session - unreachable, refused login,
//! untrusted certificate - with a stable code each. [`FtpVfs::connect`](super::FtpVfs::connect)
//! returns it directly; a `Vfs` call that has to reconnect carries it inside
//! [`VfsError::Io`], and [`FtpError::from_vfs`] gets it back out.
//!
//! [`Failure`] is one command's outcome inside a session: the server's refusal (a reply code,
//! the connection still good) or a broken connection. Refusals are mapped to `VfsError` kinds
//! here; the backend refines the ambiguous `550` by looking at the path first. A command the
//! server does not implement (`500`, `502`, `504`) is `unsupported` with the command as its
//! operation, and [`unsupported_code`] names it for the person: "The server does not support
//! APPE." - never with its argument.

use std::{io, path::Path};

use rc_vfs::VfsError;

use super::tls::CertificateRejection;

/// The FTPS certificate is not one the system roots vouch for, and nothing trusts it.
pub const CERTIFICATE_UNKNOWN: &str = "certificate_unknown";
/// The server cannot be reached, or closed the connection before it was ready.
pub const FTP_CONNECT_FAILED: &str = "ftp_connect_failed";
/// The server refused the user name or password.
pub const FTP_LOGIN_FAILED: &str = "ftp_login_failed";
/// The server did not answer within the timeout.
pub const FTP_TIMEOUT: &str = "ftp_timeout";
/// The server refused `AUTH TLS`: it offers no explicit FTPS.
pub const FTP_TLS_REFUSED: &str = "ftp_tls_refused";
/// The TLS handshake failed for a reason other than trust.
pub const FTP_TLS_FAILED: &str = "ftp_tls_failed";
/// The server answered something the protocol does not allow at that point.
pub const FTP_PROTOCOL: &str = "ftp_protocol";
/// The server takes no further session from this user or address (`421`, "too many users").
pub const FTP_TOO_MANY_SESSIONS: &str = "ftp_too_many_sessions";

/// Why no working FTP session came about.
#[derive(Debug, thiserror::Error)]
pub enum FtpError {
    /// The certificate failed the system roots and the trust hook refused it.
    #[error("the FTP server's certificate is not trusted ({}): SHA-256 {}", .0.reason, .0.fingerprint)]
    CertificateUnknown(Box<CertificateRejection>),
    #[error("cannot reach the FTP server: {0}")]
    Connect(#[source] io::Error),
    #[error("the FTP server refused the login")]
    LoginFailed,
    #[error("the FTP server did not answer in time")]
    Timeout,
    #[error("the FTP server does not offer TLS (AUTH TLS refused)")]
    TlsRefused,
    #[error("the TLS handshake with the FTP server failed: {0}")]
    Tls(String),
    #[error("the FTP server answered unexpectedly: {0}")]
    Protocol(String),
    /// The server refused one more session (`421`, or `530` naming its maximum); the pool
    /// waits for one of its own where it has any ([`super::pool`]).
    #[error("the FTP server takes no further session: {0}")]
    TooManySessions(String),
}

impl FtpError {
    /// Every code, in declaration order.
    pub const ALL_CODES: &[&str] = &[
        CERTIFICATE_UNKNOWN,
        FTP_CONNECT_FAILED,
        FTP_LOGIN_FAILED,
        FTP_TIMEOUT,
        FTP_TLS_REFUSED,
        FTP_TLS_FAILED,
        FTP_PROTOCOL,
        FTP_TOO_MANY_SESSIONS,
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::CertificateUnknown(_) => CERTIFICATE_UNKNOWN,
            Self::Connect(_) => FTP_CONNECT_FAILED,
            Self::LoginFailed => FTP_LOGIN_FAILED,
            Self::Timeout => FTP_TIMEOUT,
            Self::TlsRefused => FTP_TLS_REFUSED,
            Self::Tls(_) => FTP_TLS_FAILED,
            Self::Protocol(_) => FTP_PROTOCOL,
            Self::TooManySessions(_) => FTP_TOO_MANY_SESSIONS,
        }
    }

    /// The session failure inside a `VfsError` a reconnecting `Vfs` call returned.
    #[must_use]
    pub fn from_vfs(error: &VfsError) -> Option<&Self> {
        match error {
            VfsError::Io { source, .. } => source.get_ref()?.downcast_ref::<Self>(),
            _ => None,
        }
    }

    /// Wraps the failure for a `Vfs` call on `path`.
    pub(crate) fn into_vfs(self, path: &Path) -> VfsError {
        let kind = match &self {
            Self::Timeout => io::ErrorKind::TimedOut,
            Self::Connect(error) => error.kind(),
            Self::CertificateUnknown(_) | Self::LoginFailed => io::ErrorKind::PermissionDenied,
            Self::TlsRefused | Self::Tls(_) | Self::Protocol(_) | Self::TooManySessions(_) => {
                io::ErrorKind::Other
            }
        };
        VfsError::Io {
            path: path.to_path_buf(),
            source: io::Error::new(kind, self),
        }
    }
}

/// The commands whose refusal as not implemented a person can meet, each with the stable code
/// that names it ([`unsupported_code`]). `MLSD`, `MLST`, `SIZE` and `MDTM` are missing on
/// purpose: their refusal is worked around, never shown. `PASV` stands for both passive modes,
/// refused one after the other.
pub const UNSUPPORTED_COMMANDS: &[(&str, &str)] = &[
    ("APPE", "ftp_unsupported_appe"),
    ("CWD", "ftp_unsupported_cwd"),
    ("DELE", "ftp_unsupported_dele"),
    ("EPRT", "ftp_unsupported_eprt"),
    ("LIST", "ftp_unsupported_list"),
    ("MFMT", "ftp_unsupported_mfmt"),
    ("MKD", "ftp_unsupported_mkd"),
    ("PASV", "ftp_unsupported_passive"),
    ("PORT", "ftp_unsupported_port"),
    ("REST", "ftp_unsupported_rest"),
    ("RETR", "ftp_unsupported_retr"),
    ("RMD", "ftp_unsupported_rmd"),
    ("RNFR", "ftp_unsupported_rename"),
    ("SITE CHMOD", "ftp_unsupported_chmod"),
    ("STOR", "ftp_unsupported_stor"),
];

/// The code that names the FTP command an `unsupported` error is about, if it is one of
/// [`UNSUPPORTED_COMMANDS`].
#[must_use]
pub fn unsupported_code(error: &VfsError) -> Option<&'static str> {
    let VfsError::Unsupported { operation, .. } = error else {
        return None;
    };
    UNSUPPORTED_COMMANDS
        .iter()
        .find(|(command, _)| command == operation)
        .map(|(_, code)| *code)
}

/// One command's outcome when it did not succeed.
#[derive(Debug)]
pub(crate) enum Failure {
    /// The server refused `command` (its name, never its argument); the connection stays usable.
    Reply {
        command: &'static str,
        code: u32,
        text: String,
    },
    /// The connection broke or timed out and was discarded.
    Broken(io::Error),
}

impl Failure {
    /// `error`, the outcome of `command`.
    pub fn of(command: &'static str, error: suppaftp::FtpError) -> Self {
        match error {
            suppaftp::FtpError::UnexpectedResponse(response) => Self::Reply {
                command,
                code: response.status.code(),
                text: reply_text(&response.body),
            },
            suppaftp::FtpError::ConnectionError(source) => Self::Broken(source),
            other => Self::Broken(io::Error::other(other.to_string())),
        }
    }

    /// Whether the server does not know the command at all (`500`, `502`, `504`).
    pub fn not_implemented(&self) -> bool {
        matches!(self.reply_code(), Some(500 | 502 | 504))
    }

    /// The reply code of a refusal.
    pub fn reply_code(&self) -> Option<u32> {
        match self {
            Self::Reply { code, .. } => Some(*code),
            Self::Broken(_) => None,
        }
    }

    /// The kind of `VfsError` this failure is on `path`, without looking at the path.
    ///
    /// `550` ("file unavailable") is what servers answer for a missing path and for a refused
    /// one alike; the text decides, `not_found` without a hint of permissions. The backend
    /// checks the path before it falls back to this.
    pub fn into_vfs(self, path: &Path) -> VfsError {
        let path_buf = path.to_path_buf();
        match self {
            Self::Broken(source) => VfsError::Io {
                path: path_buf,
                source,
            },
            Self::Reply {
                command,
                code,
                text,
            } => match code {
                530 | 532 => VfsError::PermissionDenied { path: path_buf },
                550 if mentions_permission(&text) => VfsError::PermissionDenied { path: path_buf },
                550 => VfsError::NotFound { path: path_buf },
                553 => VfsError::InvalidPath {
                    path: path_buf,
                    reason: "the server does not allow this name",
                },
                500 | 502 | 504 => VfsError::Unsupported {
                    path: path_buf,
                    operation: command,
                },
                _ => VfsError::Io {
                    path: path_buf,
                    source: io::Error::other(format!("FTP {code}: {text}")),
                },
            },
        }
    }
}

/// Whether a refusal's text speaks of permissions rather than absence.
fn mentions_permission(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    ["permission", "denied", "access", "not allowed", "forbidden"]
        .iter()
        .any(|word| lower.contains(word))
}

/// The reply text of a suppaftp response, without its code.
pub(crate) fn reply_text(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    let text = text.trim();
    text.get(4..).unwrap_or(text).trim().to_owned()
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
