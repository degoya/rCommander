//! The names of a device path and the entries of its objects, for [`super::DeviceFs`].

use std::path::{Component, Path, PathBuf};

use super::{Object, cache::same_name};
use crate::{Attributes, Entry, EntryKind, Result, VfsError};

/// The names of a device path, from its root.
pub(super) fn names(path: &Path) -> Result<Vec<String>> {
    let invalid = || VfsError::InvalidPath {
        path: path.to_path_buf(),
        reason: "a device path is a list of names from its root",
    };
    let mut names = Vec::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(name) => names.push(name.to_str().ok_or_else(invalid)?.to_owned()),
            Component::Prefix(_) | Component::ParentDir => return Err(invalid()),
        }
    }
    Ok(names)
}

/// The path of `names` from the root.
pub(super) fn path_of(names: &[String]) -> PathBuf {
    let mut path = PathBuf::from("/");
    path.extend(names);
    path
}

/// The entry of `object`: a dot file is hidden, as on the phone's own Linux.
pub(super) fn entry(object: &Object) -> Entry {
    Entry {
        name: object.name.clone(),
        kind: if object.folder {
            EntryKind::Dir
        } else {
            EntryKind::File
        },
        size: if object.folder { 0 } else { object.size },
        modified: object.modified,
        created: None,
        attributes: Attributes::default(),
        hidden: object.name.starts_with('.'),
        owner: None,
    }
}

/// What the device cannot do, refused on `path` before it is asked.
pub(super) fn unsupported(path: &Path, operation: &'static str) -> VfsError {
    VfsError::Unsupported {
        path: path.to_path_buf(),
        operation,
    }
}

/// The object `name` of `objects`: of that very name, else of it in another case.
pub(super) fn find(objects: &[Object], name: &str) -> Option<Object> {
    objects
        .iter()
        .find(|object| object.name == name)
        .or_else(|| objects.iter().find(|object| same_name(&object.name, name)))
        .cloned()
}
