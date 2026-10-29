//! The virtual file system: one [`Vfs`] trait every panel, operation and MCP tool works
//! against, whatever holds the files.
//!
//! A location is a [`VfsPath`]: the [`BackendId`] that names the file system and the path inside
//! it. The engine keeps one `Arc<dyn Vfs>` per backend and hands it the `path` part; a backend
//! never sees another backend's paths. [`LocalFs`] covers the machine - and WSL distributions on
//! Windows, which are plain UNC shares (`\\wsl.localhost\<distro>\...`, see [`wsl_unc_path`]);
//! [`wsl`] finds the installed distributions. [`ArchiveFs`] is the inside of an archive file on
//! any backend. A remote backend carries the id of its connection: FTP and FTPS
//! ([`BackendId::Ftp`]) and SFTP ([`BackendId::Sftp`]), both served by `rc-remote`.
//! Every backend passes the same [`conformance`] suite (the `conformance` feature); a read-only
//! one, such as an archive, its read-only part. `MemoryFs` (the `memory` feature) is a whole file
//! system in memory for the tests of crates that work against [`Vfs`]; `testing` (the same
//! feature) has what such tests wrap a backend with. [`TimeoutFs`] holds any
//! backend to a time limit and lets its waiting calls be stopped: a share that does not answer
//! never holds the caller (RC-080-04).

mod archive;
#[cfg(any(test, feature = "conformance"))]
pub mod conformance;
mod entry;
mod error;
mod local;
#[cfg(any(test, feature = "memory"))]
mod memory;
mod paths;
mod props;
#[cfg(any(test, feature = "memory"))]
pub mod testing;
mod timeout;
mod vfs;
pub mod wsl;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub use archive::{
    ArchiveEntryInfo, ArchiveFormat, ArchiveFs, ArchiveItem, ArchiveLimits, ArchivePassword,
    ArchiveSettings, Compression, UnsafeName, detect_format, detect_format_within,
};
pub use entry::{
    Attributes, Capabilities, Entry, EntryKind, Owner, SetAttributes, TargetKind, WindowsFlags,
    unix_seconds, unix_time,
};
pub use error::{Result, VfsError};
pub use local::LocalFs;
pub use local::space::disk_space;
#[cfg(any(test, feature = "memory"))]
pub use memory::MemoryFs;
pub use paths::{
    NetworkPath, WslPath, drive_letter, long_path, network_path, parse_wsl_unc, split_name,
    wsl_unc_path,
};
pub use props::{Details, FileId, LinkKind, LinkKinds, PropertyCapabilities, PropertyChange};
pub use timeout::{
    DEFAULT_LISTING_TIMEOUT, DEFAULT_REQUEST_TIMEOUT, IoCancel, TimeoutFs, Timeouts,
};
pub use vfs::{ReadSeek, ReadStream, SeekStream, Vfs, WriteMode, WriteStream};
pub use wsl::{WslCatalog, WslDistro};

/// Which file system a [`VfsPath`] lives on.
///
/// One variant per backend; a remote backend will carry the id of its connection.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum BackendId {
    /// The machine's own file systems, network shares and, on Windows, WSL distributions.
    Local,
    /// A [`MemoryFs`] of a test, by number: tests that need two non-local backends.
    #[cfg(any(test, feature = "memory"))]
    Memory(u32),
    /// The inside of an archive file, which lies on another backend.
    Archive {
        /// Where the archive file is.
        outer: Box<VfsPath>,
        /// What kind of archive it is.
        format: ArchiveFormat,
    },
    /// An FTP or FTPS server: the connection of the connection tree that reaches it. Shown as
    /// `ftp://<connection name>/<path>`; the path is absolute on the server.
    Ftp {
        /// The connection's id in `rcommander.db`.
        connection_id: i64,
    },
    /// An SSH server's file system over SFTP: the SSH connection of the connection tree that
    /// reaches it. Shown as `sftp://<connection name>/<path>`; the path is absolute (POSIX) on
    /// the server.
    Sftp {
        /// The connection's id in `rcommander.db`.
        connection_id: i64,
    },
}

/// A location: the backend and the path inside it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VfsPath {
    /// The file system the path belongs to.
    pub backend: BackendId,
    /// The path as that backend understands it.
    pub path: PathBuf,
}

impl VfsPath {
    /// A path on the local file system.
    #[must_use]
    pub fn local(path: impl Into<PathBuf>) -> Self {
        Self {
            backend: BackendId::Local,
            path: path.into(),
        }
    }

    /// The location `path` inside the archive file `outer` (`/` is the archive's root).
    #[must_use]
    pub fn archive(outer: VfsPath, format: ArchiveFormat, path: impl Into<PathBuf>) -> Self {
        Self {
            backend: BackendId::Archive {
                outer: Box::new(outer),
                format,
            },
            path: path.into(),
        }
    }

    /// The child `name` of this location, on the same backend.
    #[must_use]
    pub fn join(&self, name: impl AsRef<std::path::Path>) -> Self {
        Self {
            backend: self.backend.clone(),
            path: self.path.join(name),
        }
    }

    /// The parent location: at the root of an archive the directory that holds the archive
    /// file, as `..` leaves an archive in Total Commander; `None` at the root of any other
    /// backend.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        match (self.path.parent(), &self.backend) {
            (Some(parent), _) => Some(Self {
                backend: self.backend.clone(),
                path: parent.to_path_buf(),
            }),
            (None, BackendId::Archive { outer, .. }) => outer.parent(),
            (None, _) => None,
        }
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
