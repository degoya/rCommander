//! Archives as panel locations (RC-030-02): the archive backends the engine has open, the text
//! form of a location inside one, and the way from a typed path through an archive file into it.
//!
//! An archive is a backend of its own ([`rc_vfs::ArchiveFs`], read-only), opened on first use
//! and registered with the operations queue, so F5 out of an archive is a copy like any other.
//! At most [`KEEP_OPEN`] stay open, each with its cached listing; the oldest goes first. Nested
//! archives are not opened. Each opens with the decoder memory of `archive.decoder_memory_mib`
//! (RC-080-35); when the setting changes, the open ones are closed and open anew under it.

use std::{
    collections::{BTreeMap, VecDeque},
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use rc_ops::Backend;
use rc_vfs::{
    ArchiveEntryInfo, ArchiveFs, ArchiveLimits, ArchiveSettings, BackendId, Vfs, VfsError, VfsPath,
    detect_format_within,
};
use serde_json::Value;

use crate::{
    Engine, EngineError,
    settings::{DECODER_MEMORY_KEY, stored_setting},
};

/// Archives kept open at once.
const KEEP_OPEN: usize = 8;

/// File names Enter opens as an archive while `archive.enter_opens` is on; Ctrl+PageDown opens
/// any file whose content is one. Enter on a `.docx` or a `.jar` (zip inside) goes to the system.
const ENTER_EXTENSIONS: &[&str] = &[
    ".zip", ".7z", ".tar", ".tgz", ".tbz", ".tbz2", ".txz", ".tzst", ".gz", ".bz2", ".xz", ".zst",
];

/// What an archive says about the entries of one directory, by name: packed size, encrypted,
/// an unsafe stored name.
pub type ArchiveInfo = BTreeMap<String, ArchiveEntryInfo>;

/// The archive backends the engine opened, the one used last first, and the decoder memory in
/// bytes they open with.
pub(crate) struct ArchiveCatalog {
    open: Mutex<VecDeque<(BackendId, Arc<ArchiveFs>)>>,
    decoder_memory: AtomicU64,
}

impl Default for ArchiveCatalog {
    fn default() -> Self {
        Self {
            open: Mutex::default(),
            decoder_memory: AtomicU64::new(ArchiveLimits::default().max_decoder_memory),
        }
    }
}

impl ArchiveCatalog {
    fn lock(&self) -> MutexGuard<'_, VecDeque<(BackendId, Arc<ArchiveFs>)>> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The limits an archive opens with: the defaults, the decoder memory as set.
    pub(crate) fn limits(&self) -> ArchiveLimits {
        ArchiveLimits {
            max_decoder_memory: self.decoder_memory.load(Ordering::Relaxed),
            ..ArchiveLimits::default()
        }
    }
}

/// Whether Enter opens the file `name` as an archive (by its extension).
pub(crate) fn opens_on_enter(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    ENTER_EXTENSIONS
        .iter()
        .any(|extension| name.len() > extension.len() && name.ends_with(extension))
}

/// A location as the window shows it and a person types it: inside an archive, the archive
/// file's path with the path inside appended (`C:\data\backup.zip\docs`).
#[must_use]
pub fn shown_path(path: &VfsPath) -> PathBuf {
    match &path.backend {
        BackendId::Archive { outer, .. } => {
            let mut shown = shown_path(outer);
            shown.extend(
                path.path
                    .components()
                    .filter(|component| matches!(component, Component::Normal(_))),
            );
            shown
        }
        _ => path.path.clone(),
    }
}

/// The archive file a location inside an archive lies in; `None` outside one.
#[must_use]
pub fn archive_file(path: &VfsPath) -> Option<&VfsPath> {
    match &path.backend {
        BackendId::Archive { outer, .. } => Some(outer),
        _ => None,
    }
}

/// The name of the entry a location is in its parent: at the root of an archive, the archive
/// file's name.
pub(crate) fn entry_name(path: &VfsPath) -> Option<String> {
    let name = match (&path.backend, path.path.file_name()) {
        (_, Some(name)) => name,
        (BackendId::Archive { outer, .. }, None) => outer.path.file_name()?,
        (_, None) => return None,
    };
    Some(name.to_string_lossy().into_owned())
}

/// The refusal of an archive inside an archive.
pub(crate) fn nested(path: &Path) -> EngineError {
    VfsError::Unsupported {
        path: path.to_path_buf(),
        operation: "opening an archive inside an archive",
    }
    .into()
}

impl Engine {
    /// The archive backend `backend` names, opened when it is not yet.
    pub(crate) fn archive_fs(&self, backend: &BackendId) -> Result<Arc<ArchiveFs>, EngineError> {
        let BackendId::Archive { outer, format } = backend else {
            return Err(EngineError::BackendUnavailable);
        };
        {
            let mut open = self.inner.archives.lock();
            if let Some(at) = open.iter().position(|(id, _)| id == backend)
                && let Some(found) = open.remove(at)
            {
                let fs = Arc::clone(&found.1);
                open.push_front(found);
                return Ok(fs);
            }
        }
        if matches!(outer.backend, BackendId::Archive { .. }) {
            return Err(nested(&outer.path));
        }
        let outer_vfs = self.vfs(outer)?;
        let settings = ArchiveSettings {
            limits: self.inner.archives.limits(),
            ..ArchiveSettings::default()
        };
        let fs = Arc::new(ArchiveFs::new(
            outer_vfs,
            outer.path.clone(),
            *format,
            settings,
        ));
        let as_vfs: Arc<dyn Vfs> = Arc::<ArchiveFs>::clone(&fs);
        self.inner
            .ops
            .register(backend.clone(), Backend::new(as_vfs));
        let mut open = self.inner.archives.lock();
        open.retain(|(id, _)| id != backend);
        open.push_front((backend.clone(), Arc::clone(&fs)));
        while open.len() > KEEP_OPEN {
            if let Some((id, _)) = open.pop_back() {
                self.inner.ops.unregister(&id);
            }
        }
        Ok(fs)
    }

    /// What the archive says about each entry of the directory `path`; `None` outside an
    /// archive, or when the archive cannot be read (its listing reported that already).
    pub(crate) async fn archive_info(&self, path: &VfsPath) -> Option<Arc<ArchiveInfo>> {
        if !matches!(path.backend, BackendId::Archive { .. }) {
            return None;
        }
        let fs = self.archive_fs(&path.backend).ok()?;
        let items = fs.list_detailed(&path.path).await.ok()?;
        Some(Arc::new(
            items
                .into_iter()
                .map(|item| (item.entry.name, item.info))
                .collect(),
        ))
    }

    /// The root of the archive `file`; `None` when the file is no archive this backend reads.
    pub(crate) async fn archive_root(
        &self,
        file: &VfsPath,
    ) -> Result<Option<VfsPath>, EngineError> {
        if matches!(file.backend, BackendId::Archive { .. }) {
            return Err(nested(&file.path));
        }
        let vfs = self.vfs(file)?;
        self.read_archive_limits().await;
        let limits = self.inner.archives.limits();
        let format = detect_format_within(&vfs, &file.path, limits).await?;
        Ok(format.map(|format| VfsPath::archive(file.clone(), format, "/")))
    }

    /// Takes `archive.decoder_memory_mib` as stored (RC-080-35). A changed value closes the
    /// archives open under the old one, so the next look at them opens them anew under it; a
    /// job already running out of one keeps its backend. The queue's unpack and test jobs take
    /// it from the next one on. Without a database the default holds.
    pub(crate) async fn read_archive_limits(&self) {
        let Ok(db) = self.db() else {
            return;
        };
        let default_mib = ArchiveLimits::default().max_decoder_memory >> 20;
        let mib = stored_setting(db, DECODER_MEMORY_KEY)
            .await
            .as_ref()
            .and_then(Value::as_u64)
            .unwrap_or(default_mib);
        let bytes = mib.saturating_mul(1 << 20);
        let catalog = &self.inner.archives;
        if catalog.decoder_memory.swap(bytes, Ordering::Relaxed) == bytes {
            return;
        }
        // Unpacking and the archive test read their archives under the same limit.
        self.inner.ops.set_archive_limits(catalog.limits());
        let closed: Vec<BackendId> = catalog.lock().drain(..).map(|(id, _)| id).collect();
        for id in closed {
            self.inner.ops.unregister(&id);
        }
    }

    /// `path` read as a path through an archive file (`C:\data\backup.zip\docs`): the location
    /// inside the archive when the last existing thing on the way is an archive file. `None`
    /// when it is a directory or a file of another kind. Asked only after `path` failed to list.
    pub(crate) async fn through_archive(
        &self,
        path: &VfsPath,
    ) -> Result<Option<VfsPath>, EngineError> {
        if matches!(path.backend, BackendId::Archive { .. }) {
            return Ok(None);
        }
        let vfs = self.vfs(path)?;
        for candidate in path.path.ancestors() {
            match vfs.stat(candidate).await {
                Ok(entry) if entry.is_dir_like() => return Ok(None),
                Ok(_) => {
                    let file = VfsPath {
                        backend: path.backend.clone(),
                        path: candidate.to_path_buf(),
                    };
                    let Some(root) = self.archive_root(&file).await? else {
                        return Ok(None);
                    };
                    let rest = path.path.strip_prefix(candidate).unwrap_or(Path::new(""));
                    return Ok(Some(root.join(rest)));
                }
                Err(VfsError::NotFound { .. } | VfsError::NotADirectory { .. }) => {}
                Err(_) => return Ok(None),
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
#[path = "archives_tests.rs"]
pub(crate) mod tests;
