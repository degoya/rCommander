//! The recycle bin.
//!
//! [`Vfs`](rc_vfs::Vfs) has no recycle bin - archives and servers have none - so the queue holds
//! one per backend beside it ([`crate::Backend::trash`]); a backend without one deletes only
//! permanently, after the caller has asked the person (`trash_unavailable`).
//!
//! **Decision (RC-010-06): the `trash` crate, not `IFileOperation` of our own.** Both end in the
//! same call: `trash` 5 drives `IFileOperation` with `FOF_ALLOWUNDO` itself (`SHFileOperation`
//! is gone), so a deleted entry lands in the recycle bin and can be restored from it. Compared:
//! - *Long paths*: the shell parses no `\\?\` paths; `trash` turns a verbatim path back into
//!   `C:\...` or `\\host\share\...`. Past 260 characters the shell itself may refuse - that comes
//!   back as `trash_failed` for the one entry, and the caller can delete it permanently. Our own
//!   `IFileOperation` would hit the same shell limit.
//! - *Network drives*: they have no recycle bin, and with `FOF_NO_UI` the shell deletes there
//!   *permanently without asking*. So [`SystemTrash`] refuses network paths before the shell sees
//!   them: UNC paths, mapped drives (canonicalised to UNC) and WSL distributions
//!   (`\\wsl.localhost\...`) fail with `trash_unavailable`.
//! - *Large amounts*: one `IFileOperation` per top-level entry; the shell recurses itself, so a
//!   folder of 100 000 files is one call. The queue's progress moves per top-level entry.
//! - *`unsafe`*: COM needs it; in the crate it stays out of rCommander, where `unsafe` is denied.
//!
//! On macOS the same crate moves an entry to the Trash through `NSFileManager`
//! (`trash/macos.rs`, RC-100-44). Other platforms have no recycle bin here yet
//! ([`SystemTrash::for_this_platform`] is `None`).

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

use std::{path::Path, sync::Arc};

use async_trait::async_trait;

use crate::OpsError;

/// A recycle bin for the paths of one backend.
#[async_trait]
pub trait Trash: Send + Sync {
    /// Moves the entry at `path`, with its contents, into the recycle bin.
    ///
    /// # Errors
    ///
    /// `trash_unavailable` where this path has no recycle bin, `trash_failed` when the bin
    /// refused, a file-system code when the path cannot be reached.
    async fn trash(&self, path: &Path) -> Result<(), OpsError>;
}

/// The system's recycle bin for the local backend.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemTrash;

impl SystemTrash {
    /// The recycle bin of this platform, where rCommander supports one: Windows and macOS.
    #[must_use]
    pub fn for_this_platform() -> Option<Arc<dyn Trash>> {
        if cfg!(any(windows, target_os = "macos")) {
            Some(Arc::new(Self))
        } else {
            None
        }
    }
}

#[async_trait]
impl Trash for SystemTrash {
    #[cfg(windows)]
    async fn trash(&self, path: &Path) -> Result<(), OpsError> {
        let path = path.to_path_buf();
        let shown = path.clone();
        tokio::task::spawn_blocking(move || windows::trash(&path))
            .await
            .map_err(|join| OpsError::from_io(std::io::Error::other(join), shown))?
    }

    #[cfg(target_os = "macos")]
    async fn trash(&self, path: &Path) -> Result<(), OpsError> {
        let path = path.to_path_buf();
        let shown = path.clone();
        tokio::task::spawn_blocking(move || macos::trash(&path))
            .await
            .map_err(|join| OpsError::from_io(std::io::Error::other(join), shown))?
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    async fn trash(&self, path: &Path) -> Result<(), OpsError> {
        Err(OpsError::TrashUnavailable {
            path: path.to_path_buf(),
        })
    }
}

/// Whether `path` (canonicalised) lies on a network share, which has no recycle bin.
#[cfg(any(windows, test))]
#[must_use]
pub(crate) fn on_network(path: &Path) -> bool {
    use std::path::{Component, Prefix};

    matches!(
        path.components().next(),
        Some(Component::Prefix(prefix))
            if matches!(prefix.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..))
    )
}

#[cfg(test)]
#[path = "trash_tests.rs"]
mod tests;
