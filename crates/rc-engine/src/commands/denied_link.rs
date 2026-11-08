//! A link whose listing is refused, followed to its target (RC-100-92), as Total Commander does
//! with Windows' compatibility junctions: `C:\Users\<user>\Anwendungsdaten` (German; `Application
//! Data` in English) leads to `AppData\Roaming`, and its ACL denies listing it - TC lists the
//! target instead, and the panel shows the target's path.
//!
//! Only a junction or a symbolic link to a folder on this machine is followed, only when listing
//! it was refused, and only one step: a plain folder that is denied, a dangling link (`not_found`)
//! and every other failure keep their error. On Windows only, where such links exist; elsewhere
//! listing a link lists its target already.

use std::path::{Path, PathBuf};

use rc_vfs::{BackendId, VfsError, VfsPath};

use crate::{Engine, EngineError};

/// The folder to show in place of `path`, whose listing failed with `error`, when `path` is a
/// link to `target` (as `read_link` gives it): the target as a local path, relative ones taken
/// from the link's folder, `\\?\` dropped; `None` for every other case.
pub(crate) fn follow_target(
    path: &VfsPath,
    error: &EngineError,
    target: Option<PathBuf>,
) -> Option<VfsPath> {
    let denied = matches!(error, EngineError::Vfs(VfsError::PermissionDenied { .. }));
    if !denied || path.backend != BackendId::Local {
        return None;
    }
    let target = plain(&target?);
    let target = if target.is_absolute() {
        target
    } else {
        path.path.parent()?.join(target)
    };
    (target != path.path).then(|| VfsPath::local(target))
}

/// `path` without the `\\?\` prefix `read_link` gives a junction's target (`\\?\UNC\` a share's).
fn plain(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(share) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{share}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path.to_path_buf(),
    }
}

impl Engine {
    /// The target to show in place of `path`, whose listing failed with `error`
    /// ([`follow_target`]); `None` off Windows.
    pub(crate) async fn denied_link_target(
        &self,
        path: &VfsPath,
        error: &EngineError,
    ) -> Option<VfsPath> {
        if !cfg!(windows) || path.backend != BackendId::Local {
            return None;
        }
        let target = tokio::fs::read_link(&path.path).await.ok();
        follow_target(path, error, target)
    }
}

#[cfg(test)]
#[path = "denied_link_tests.rs"]
mod tests;
