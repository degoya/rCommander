//! SFTP as a [`rc_vfs::Vfs`], on russh-sftp over the channels of an SSH session (RC-060-02).
//!
//! [`SftpVfs::connect`] takes two channels of one [`crate::SshSession`] - the engine opens them
//! with `Engine::ssh_channel`, so SFTP shares the connection with the terminal and commands - and
//! starts the `sftp` subsystem on each: one carries the requests on names (listings, `stat`,
//! `mkdir`, `rename`, removals, attributes), the other the file streams, so a long transfer never
//! holds up a listing. The home directory (`realpath(".")`) is read once and is where a panel
//! starts. Whether the server folds case is probed once on the home directory (`case.rs`,
//! [`SftpCase`]), unless the caller says.
//!
//! Paths are absolute POSIX paths on the server, whatever the local platform writes; `..` is not
//! resolved here and a name holding NUL is refused before it is sent.
//!
//! **The server is not trusted.** A listing is hostile input: a name that is empty, `.`, `..`,
//! or holds `/`, `\`, NUL, CR or LF is not an entry of the directory and is dropped; a directory
//! may list at most [`SftpLimits::max_entries`] names, and the whole listing must end within
//! [`SftpLimits::listing_timeout`]. Every request waits at most
//! [`SftpLimits::request_timeout`] (`sftp_timeout`); a closed channel is `connection_lost`, and
//! the backend then counts as closed ([`SftpVfs::is_closed`]) so the engine opens a new one - also
//! when a file stream finds the channel closed (`stream.rs`), so an interrupted transfer resumes on
//! a new session (RC-060-07). The server's own error texts never reach a message.

mod case;
mod change;
mod error;
mod listing;
mod owner;
pub(crate) mod path;
mod stream;
mod vfs;

use std::time::Duration;

pub use case::SftpCase;
pub use error::{SFTP_LISTING_TOO_LARGE, SFTP_PROTOCOL, SFTP_TIMEOUT, SFTP_UNAVAILABLE, SftpError};
pub use vfs::SftpVfs;

/// How long one request may wait for its answer unless the caller says otherwise.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a whole listing may take unless the caller says otherwise.
pub const DEFAULT_LISTING_TIMEOUT: Duration = Duration::from_secs(300);
/// How many names one directory may list unless the caller says otherwise.
pub const DEFAULT_MAX_ENTRIES: usize = 500_000;

/// The bounds a server is held to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SftpLimits {
    /// The longest wait for one answer; whole seconds, at least one.
    pub request_timeout: Duration,
    /// The longest a listing may take, however many answers it needs.
    pub listing_timeout: Duration,
    /// The most names one directory may list, the dropped ones included.
    pub max_entries: usize,
}

impl Default for SftpLimits {
    fn default() -> Self {
        Self {
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            listing_timeout: DEFAULT_LISTING_TIMEOUT,
            max_entries: DEFAULT_MAX_ENTRIES,
        }
    }
}

impl SftpLimits {
    /// The request timeout in whole seconds, as russh-sftp takes it.
    pub(crate) fn request_secs(&self) -> u64 {
        self.request_timeout.as_secs().max(1)
    }
}
