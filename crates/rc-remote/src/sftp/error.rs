//! What goes wrong with an SFTP server, in two layers, as for FTP.
//!
//! [`SftpError`] is the failure of the SFTP session itself - no subsystem, a timeout, a closed
//! channel, a broken answer, a listing past its bounds - with a stable code each. A `Vfs` call
//! carries it inside [`VfsError::Io`]; [`SftpError::from_vfs`] gets it back out.
//!
//! A refusal of one request is the server's status code. Version 3 of the protocol has few: a
//! missing path and one that is not a directory are both "no such file", and "already exists",
//! "is a directory" and "not empty" are all a bare "failure". The backend looks at the path
//! before it says which; here a status only becomes the kind it names for certain.

use std::{io, path::Path};

use rc_vfs::VfsError;
use russh_sftp::{client::error::Error, protocol::StatusCode};

use crate::ssh::CONNECTION_LOST;

/// The SSH server refused the `sftp` subsystem.
pub const SFTP_UNAVAILABLE: &str = "sftp_unavailable";
/// The SFTP server did not answer a request in time, or a listing took too long.
pub const SFTP_TIMEOUT: &str = "sftp_timeout";
/// The SFTP server answered something the protocol does not allow.
pub const SFTP_PROTOCOL: &str = "sftp_protocol";
/// A directory lists more names than a listing may hold.
pub const SFTP_LISTING_TOO_LARGE: &str = "sftp_listing_too_large";

/// Why the SFTP session failed a call.
#[derive(Debug, thiserror::Error)]
pub enum SftpError {
    #[error("the SSH server offers no SFTP")]
    Unavailable,
    #[error("the SFTP server did not answer within {seconds} s")]
    Timeout { seconds: u64 },
    #[error("the connection was lost")]
    ConnectionLost,
    #[error("the SFTP server answered unexpectedly: {reason}")]
    Protocol { reason: &'static str },
    #[error("the directory lists more than {limit} entries")]
    ListingTooLarge { limit: usize },
}

impl SftpError {
    /// Every code, in declaration order; `connection_lost` is the SSH session's code too.
    pub const ALL_CODES: &[&str] = &[
        SFTP_UNAVAILABLE,
        SFTP_TIMEOUT,
        CONNECTION_LOST,
        SFTP_PROTOCOL,
        SFTP_LISTING_TOO_LARGE,
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unavailable => SFTP_UNAVAILABLE,
            Self::Timeout { .. } => SFTP_TIMEOUT,
            Self::ConnectionLost => CONNECTION_LOST,
            Self::Protocol { .. } => SFTP_PROTOCOL,
            Self::ListingTooLarge { .. } => SFTP_LISTING_TOO_LARGE,
        }
    }

    /// The session failure inside a `VfsError` a `Vfs` call returned.
    #[must_use]
    pub fn from_vfs(error: &VfsError) -> Option<&Self> {
        match error {
            VfsError::Io { source, .. } => source.get_ref()?.downcast_ref::<Self>(),
            _ => None,
        }
    }

    /// Wraps the failure for a `Vfs` call on `path`.
    pub(crate) fn into_vfs(self, path: &Path) -> VfsError {
        let kind = match self {
            Self::Timeout { .. } => io::ErrorKind::TimedOut,
            Self::ConnectionLost => io::ErrorKind::ConnectionAborted,
            Self::Unavailable | Self::Protocol { .. } | Self::ListingTooLarge { .. } => {
                io::ErrorKind::Other
            }
        };
        VfsError::Io {
            path: path.to_path_buf(),
            source: io::Error::new(kind, self),
        }
    }

    /// The session failure a client error stands for; `None` for a refusal of the request.
    pub(crate) fn of_client(error: &Error, timeout_secs: u64) -> Option<Self> {
        match error {
            Error::Status(status) => match status.status_code {
                StatusCode::NoConnection | StatusCode::ConnectionLost => Some(Self::ConnectionLost),
                _ => None,
            },
            Error::Timeout => Some(Self::Timeout {
                seconds: timeout_secs,
            }),
            // The session's task ended: the channel is closed, or its stream failed.
            Error::IO(_) | Error::UnexpectedBehavior(_) => Some(Self::ConnectionLost),
            Error::UnexpectedPacket => Some(Self::Protocol {
                reason: "an answer of the wrong type",
            }),
            Error::Limited(_) => Some(Self::Protocol {
                reason: "a request over the server's own limits",
            }),
        }
    }
}

/// The `VfsError` of a failed request on `path`: the session failure, or the kind the status
/// names for certain. The server's message text is left out; it is the other side's free text.
pub(crate) fn into_vfs(error: Error, path: &Path, timeout_secs: u64) -> VfsError {
    if let Some(failure) = SftpError::of_client(&error, timeout_secs) {
        return failure.into_vfs(path);
    }
    let path_buf = path.to_path_buf();
    let Error::Status(status) = error else {
        return VfsError::Io {
            path: path_buf,
            source: io::Error::other("the SFTP request failed"),
        };
    };
    match status.status_code {
        StatusCode::NoSuchFile => VfsError::NotFound { path: path_buf },
        StatusCode::PermissionDenied => VfsError::PermissionDenied { path: path_buf },
        StatusCode::OpUnsupported => VfsError::Unsupported {
            path: path_buf,
            operation: "this request on this SFTP server",
        },
        StatusCode::BadMessage => VfsError::InvalidPath {
            path: path_buf,
            reason: "the SFTP server does not accept this name",
        },
        code => VfsError::Io {
            path: path_buf,
            source: io::Error::other(format!("the SFTP server refused the request ({code})")),
        },
    }
}

/// Whether `error` is the server's "no such file".
pub(crate) fn is_no_such_file(error: &Error) -> bool {
    status_of(error) == Some(StatusCode::NoSuchFile)
}

/// Whether `error` is a refusal that may stand for several kinds ("no such file" also for "not
/// a directory", "failure" for "exists", "is a directory", "not empty"): worth a look at the path.
pub(crate) fn is_vague(error: &Error) -> bool {
    matches!(
        status_of(error),
        Some(StatusCode::NoSuchFile | StatusCode::Failure)
    )
}

/// Whether `error` is the end of a listing or a file.
pub(crate) fn is_eof(error: &Error) -> bool {
    status_of(error) == Some(StatusCode::Eof)
}

fn status_of(error: &Error) -> Option<StatusCode> {
    match error {
        Error::Status(status) => Some(status.status_code),
        _ => None,
    }
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
