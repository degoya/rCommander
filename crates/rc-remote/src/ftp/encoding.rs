//! Names on the wire: UTF-8 where the server offers it, the configured encoding otherwise.
//!
//! Decoding covers every listing, so a server without UTF-8 shows its umlauts right. Encoding is
//! narrower: suppaftp writes every command as a Rust `String`, i.e. as UTF-8, so a name outside
//! ASCII cannot be sent to a Latin-1 server at all. Such a path is refused with `unsupported`
//! instead of being sent as bytes the server would read as a different name.

use std::path::Path;

use rc_vfs::{Result, VfsError};

use super::config::NameEncoding;
use crate::wire::{Refusals, absolute};

/// The encoding one connection speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wire {
    Utf8,
    Latin1,
}

impl Wire {
    /// UTF-8 when the server offers it (`FEAT` lists `UTF8`), the configured fallback otherwise.
    pub fn negotiate(server_offers_utf8: bool, fallback: NameEncoding) -> Self {
        match (server_offers_utf8, fallback) {
            (true, _) | (false, NameEncoding::Utf8) => Self::Utf8,
            (false, NameEncoding::Latin1) => Self::Latin1,
        }
    }

    /// One line of a listing as text. Invalid UTF-8 becomes U+FFFD; Latin-1 maps every byte.
    pub fn decode(self, bytes: &[u8]) -> String {
        match self {
            Self::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
            Self::Latin1 => bytes.iter().map(|&byte| char::from(byte)).collect(),
        }
    }
}

/// What FTP says when it refuses a path.
const REFUSALS: Refusals = Refusals {
    relative: "an FTP path is absolute",
    prefix: "a drive or share is not an FTP path",
    parent: "`..` is not resolved on FTP",
};

/// The absolute FTP path (`/a/b`) of a backend path, checked for what the wire can carry.
///
/// Relative paths, `..`, Windows prefixes and names holding CR, LF or NUL (which would end or
/// smuggle a command) are invalid; a name outside ASCII on a Latin-1 connection is unsupported.
pub(crate) fn ftp_path(path: &Path, wire: Wire) -> Result<String> {
    absolute(path, &REFUSALS, |name| {
        if name.contains(['\r', '\n', '\0']) {
            return Err(VfsError::InvalidPath {
                path: path.to_path_buf(),
                reason: "a name cannot hold CR, LF or NUL",
            });
        }
        if wire == Wire::Latin1 && !name.is_ascii() {
            return Err(VfsError::Unsupported {
                path: path.to_path_buf(),
                operation: "a name outside ASCII on a server without UTF-8",
            });
        }
        Ok(())
    })
}

#[cfg(test)]
#[path = "encoding_tests.rs"]
mod tests;
