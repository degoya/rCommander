//! Archives searched like folders (RC-100-93, Total Commander's "Search archives").
//!
//! A file whose name Enter would open as an archive (`archives::opens_on_enter`) and whose first
//! bytes are one is opened as an [`ArchiveFs`] of the search's own - not through the engine's
//! catalogue, so a search does not push the panels' open archives out - with the limits and the
//! decoder memory the engine opens archives with. The walk searches its tree and drops it once
//! the last folder of it is done. Only archives on the machine's own drives (WSL included) are
//! entered: on a server every archive would cross the line in full. An archive inside an archive
//! is never entered; the limits stop a bomb, which the walk reports as skipped.

use std::{path::Path, sync::Arc};

use rc_vfs::{
    ArchiveFs, ArchiveLimits, ArchiveSettings, BackendId, Vfs, VfsError, VfsPath,
    detect_format_within,
};

/// An archive the walk entered: its backend and where its hits are.
pub(super) struct Scope {
    pub(super) vfs: Arc<dyn Vfs>,
    pub(super) backend: BackendId,
}

impl std::fmt::Debug for Scope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Scope")
            .field("backend", &self.backend)
            .finish_non_exhaustive()
    }
}

/// Whether a search may enter archives found on `backend`: the machine's own file systems only.
pub(super) fn searched_on(backend: &BackendId) -> bool {
    matches!(backend, BackendId::Local)
}

/// Whether the file `name` is one the walk looks into, by its extension.
pub(super) fn candidate(name: &str) -> bool {
    crate::archives::opens_on_enter(name)
}

/// The archive at `path` on `outer` (the backend `outer_backend`), opened within `limits`;
/// `None` when the file is no archive this backend reads.
///
/// # Errors
///
/// The file cannot be read, or its header passes the limits.
pub(super) async fn open(
    outer: &Arc<dyn Vfs>,
    outer_backend: &BackendId,
    path: &Path,
    limits: ArchiveLimits,
) -> Result<Option<Scope>, VfsError> {
    let Some(format) = detect_format_within(outer, path, limits).await? else {
        return Ok(None);
    };
    let settings = ArchiveSettings {
        limits,
        ..ArchiveSettings::default()
    };
    let fs = ArchiveFs::new(Arc::clone(outer), path.to_path_buf(), format, settings);
    let file = VfsPath {
        backend: outer_backend.clone(),
        path: path.to_path_buf(),
    };
    Ok(Some(Scope {
        vfs: Arc::new(fs),
        backend: BackendId::Archive {
            outer: Box::new(file),
            format,
        },
    }))
}
