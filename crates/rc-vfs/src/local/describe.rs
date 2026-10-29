//! Turning file-system metadata into [`Entry`] values.

use std::{
    fs::{self, Metadata},
    io,
    path::Path,
};

use super::platform::{self, Owners};
use crate::{
    Details, Entry, EntryKind, Result, TargetKind, VfsError, long_path, parse_wsl_unc,
    paths::strip_verbatim,
};

/// The entries of `dir` (the caller's spelling; the file system gets [`long_path`]).
pub(super) fn list(dir: &Path) -> Result<Vec<Entry>> {
    let mut owners = Owners::new();
    walk(dir, |name, real, meta, in_wsl| {
        describe(name, real, meta, in_wsl, &mut owners)
    })
}

/// [`list`] with each entry's [`Details`] from the same metadata.
pub(super) fn list_details(dir: &Path) -> Result<Vec<(Entry, Details)>> {
    let mut owners = Owners::new();
    walk(dir, |name, real, meta, in_wsl| {
        let details = platform::details(meta, real);
        (describe(name, real, meta, in_wsl, &mut owners), details)
    })
}

/// The details of the entry at `path`, a link's own.
pub(super) fn details(path: &Path) -> Result<Details> {
    let real = long_path(path);
    let meta = fs::symlink_metadata(&real).map_err(|error| VfsError::from_io(error, path))?;
    Ok(platform::details(&meta, &real))
}

/// One item per entry of `dir`, made by `make` from its name, path and not-followed metadata.
fn walk<T>(
    dir: &Path,
    mut make: impl FnMut(String, &Path, &Metadata, bool) -> T,
) -> Result<Vec<T>> {
    let in_wsl = parse_wsl_unc(dir).is_some();
    let read = fs::read_dir(long_path(dir)).map_err(|error| VfsError::from_io(error, dir))?;
    let mut entries = Vec::new();
    for item in read {
        let item = item.map_err(|error| VfsError::from_io(error, dir))?;
        let name = item.file_name().to_string_lossy().into_owned();
        // Not followed: a link is described as a link. On Windows this costs no extra call.
        let meta = match item.metadata() {
            Ok(meta) => meta,
            // Removed between the listing and the look: it is gone, so it is not listed.
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(VfsError::from_io(error, dir.join(&name))),
        };
        entries.push(make(name, &item.path(), &meta, in_wsl));
    }
    Ok(entries)
}

/// The entry at `path`, a link described as the link.
pub(super) fn stat(path: &Path) -> Result<Entry> {
    let real = long_path(path);
    let meta = fs::symlink_metadata(&real).map_err(|error| VfsError::from_io(error, path))?;
    let name = path.file_name().map_or_else(
        || path.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let in_wsl = path
        .parent()
        .is_some_and(|dir| parse_wsl_unc(dir).is_some());
    Ok(describe(name, &real, &meta, in_wsl, &mut Owners::new()))
}

/// One entry from its not-followed metadata. A link's size, times and attributes are its
/// target's when the target exists; its owner is the link's own, as `ls -l` shows it.
fn describe(
    name: String,
    real: &Path,
    meta: &Metadata,
    in_wsl: bool,
    owners: &mut Owners,
) -> Entry {
    let (kind, shown) = if meta.file_type().is_symlink() {
        let target = fs::read_link(real)
            .map(|target| strip_verbatim(&target))
            .unwrap_or_default();
        match fs::metadata(real) {
            Ok(followed) => (
                EntryKind::Symlink {
                    target,
                    target_kind: Some(target_kind(&followed)),
                },
                followed,
            ),
            Err(_) => (
                EntryKind::Symlink {
                    target,
                    target_kind: None,
                },
                meta.clone(),
            ),
        }
    } else if meta.is_dir() {
        (EntryKind::Dir, meta.clone())
    } else if meta.is_file() {
        (EntryKind::File, meta.clone())
    } else {
        (EntryKind::Other, meta.clone())
    };
    let hidden = platform::hidden(&name, &shown, in_wsl);
    Entry {
        size: if shown.is_file() { shown.len() } else { 0 },
        modified: shown.modified().ok(),
        created: shown.created().ok(),
        attributes: platform::attributes(&shown),
        hidden,
        kind,
        name,
        owner: owners.of(meta),
    }
}

fn target_kind(followed: &Metadata) -> TargetKind {
    if followed.is_dir() {
        TargetKind::Dir
    } else if followed.is_file() {
        TargetKind::File
    } else {
        TargetKind::Other
    }
}
