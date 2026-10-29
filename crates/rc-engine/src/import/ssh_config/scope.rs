//! How much the import reads, and where an `Include` may lead under MCP (RC-080-12).
//!
//! Every file - the configuration and each one it includes - is a regular file of at most
//! [`MAX_FILE_BYTES`]; a larger one is not read (`FileTooLarge`), a pipe is never opened.
//!
//! Under MCP ([`McpScope`]) an `Include` is followed only to a file inside the `.ssh` folder,
//! judged as it really is (links followed), and only when the MCP read guard lets it through.
//! The guard's verdict on `.ssh` itself - every file there but `config`, `known_hosts` and public
//! keys is private ([`SSH_PRIVATE_FILE`]) - is the one it does not apply: inside `.ssh` the key
//! guard of the import (`guard.rs`) is the stricter judge, and nothing of a file but five
//! keywords' values is kept. A link out of `.ssh` to a credential file or the database leads
//! outside and is refused.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::{
    import::bounded_read::read_bounded,
    read_guard::{ReadGuard, SSH_PRIVATE_FILE},
};

/// The most a configuration file may hold.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// A file's text, when it is a regular file of at most [`MAX_FILE_BYTES`] (`bounded_read`); bytes
/// that are no UTF-8 become U+FFFD instead of failing the import.
pub(super) fn read_text(path: &Path) -> io::Result<String> {
    let bytes = read_bounded(path, MAX_FILE_BYTES)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Where includes may lead when an MCP call imports.
#[derive(Debug, Clone)]
pub(crate) struct McpScope {
    /// The `.ssh` folder as it really is.
    ssh_dir: PathBuf,
    guard: Option<ReadGuard>,
}

impl McpScope {
    /// The scope of the folder `ssh_dir` (as it really is, when it exists), judged by `guard`.
    pub(crate) fn new(ssh_dir: &Path, guard: Option<ReadGuard>) -> Self {
        Self {
            ssh_dir: fs::canonicalize(ssh_dir).unwrap_or_else(|_| ssh_dir.to_path_buf()),
            guard,
        }
    }

    /// Whether an included file that really is `canonical` may be read.
    pub(crate) fn admits(&self, canonical: &Path) -> bool {
        canonical != self.ssh_dir
            && canonical.starts_with(&self.ssh_dir)
            && self.guard.is_none_or(|guard| {
                guard
                    .refuses(canonical)
                    .is_none_or(|reason| reason == SSH_PRIVATE_FILE)
            })
    }
}

#[cfg(test)]
#[path = "scope_tests.rs"]
mod tests;
