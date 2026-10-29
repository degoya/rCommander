//! Where a job may go: its sources on one backend, a copy never onto or into itself, names
//! compared as the backend compares them.

use std::path::Path;

use rc_vfs::{BackendId, VfsPath};

use crate::OpsError;

/// The one backend every source lies on.
pub(super) fn one_backend(sources: &[VfsPath]) -> Result<BackendId, OpsError> {
    let first = sources.first().ok_or(OpsError::NoSources)?;
    if sources.iter().any(|source| source.backend != first.backend) {
        return Err(OpsError::MixedSources);
    }
    Ok(first.backend.clone())
}

/// Refuses copying `source` into the directory `target` of the same backend when that puts it
/// onto itself or into itself.
pub(super) fn check_placement(
    source: &Path,
    target: &Path,
    case_sensitive: bool,
) -> Result<(), OpsError> {
    let fold = |path: &Path| fold(path, case_sensitive);
    let (source_parts, target_parts) = (fold(source), fold(target));
    if source
        .parent()
        .is_some_and(|parent| fold(parent) == target_parts)
    {
        return Err(OpsError::SameLocation {
            path: source.to_path_buf(),
        });
    }
    if target_parts.starts_with(&source_parts) {
        return Err(OpsError::TargetInsideSource {
            path: target.to_path_buf(),
        });
    }
    Ok(())
}

/// Refuses copying `source` to the path `target` of the same backend when that is the source
/// itself or lies inside it. Where case does not count, a name that differs only in case is the
/// source too - except for a move, which renames it to the new case (`case_rename.rs`).
pub(super) fn check_placement_as(
    source: &Path,
    target: &Path,
    case_sensitive: bool,
    moving: bool,
) -> Result<(), OpsError> {
    let (source_parts, target_parts) = (fold(source, case_sensitive), fold(target, case_sensitive));
    if source_parts == target_parts {
        if moving && source != target {
            return Ok(());
        }
        return Err(OpsError::SameLocation {
            path: source.to_path_buf(),
        });
    }
    if target_parts.starts_with(&source_parts) {
        return Err(OpsError::TargetInsideSource {
            path: target.to_path_buf(),
        });
    }
    Ok(())
}

/// The components of `path`, in lower case where case does not count.
pub(crate) fn fold(path: &Path, case_sensitive: bool) -> Vec<String> {
    path.components()
        .map(|component| {
            let text = component.as_os_str().to_string_lossy();
            if case_sensitive {
                text.into_owned()
            } else {
                text.to_lowercase()
            }
        })
        .collect()
}
