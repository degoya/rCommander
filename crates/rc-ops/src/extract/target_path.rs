//! Where an archive entry lands: a path built below the target from names that were each
//! checked, so it cannot leave the target whatever the archive stored.
//!
//! The archive backend hands out cleaned names already (no `..`, no root, no drive); here every
//! part must also be one plain name Windows can hold ([`crate::check_name`]): no separator, no
//! `:` (an NTFS stream), no device name, no trailing dot or space that Windows would strip into
//! another name. The joined path is held against the target once more at the end.

use std::path::{Component, Path, PathBuf};

use crate::{OpsError, check_name};

/// The path of the entry `parts` below `root`, or why it is refused.
pub(crate) fn below(root: &Path, parts: &[&str]) -> Result<PathBuf, &'static str> {
    if parts.is_empty() {
        return Err("an empty name");
    }
    let mut path = root.to_path_buf();
    for part in parts {
        match check_name(part) {
            Ok(()) => path.push(part),
            Err(OpsError::InvalidName { reason, .. }) => return Err(reason),
            Err(_) => return Err("a name the target cannot hold"),
        }
    }
    let depth = |path: &Path| path.components().count();
    let normal = path
        .components()
        .skip(depth(root))
        .all(|component| matches!(component, Component::Normal(_)));
    if !path.starts_with(root) || depth(&path) != depth(root) + parts.len() || !normal {
        return Err("a path outside the target");
    }
    Ok(path)
}

/// The parts of `rel` as text; `None` when one is not UTF-8 or not a plain name.
pub(crate) fn parts(rel: &Path) -> Option<Vec<&str>> {
    rel.components()
        .map(|component| match component {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
#[path = "target_path_tests.rs"]
mod tests;
