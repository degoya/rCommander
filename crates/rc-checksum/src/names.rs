//! Which names may be used: a name this crate creates (a checksum file, split parts, a combined
//! file), a name of an entry to hash, and a relative name read from a checksum file, which is
//! untrusted.

use std::path::{Path, PathBuf};

use crate::ChecksumError;

/// `name` when it may name a file this crate creates: one plain name by the one name rule
/// ([`rc_rename::check_plain`], RC-080-43) - the Windows rules on every file system, as a new
/// name of a multi-rename keeps them. A `.crc` file's `filename=CON`, `a:b.iso` (an NTFS stream)
/// or `report.` never becomes the name of a file, nor a split part of a listed `a:b.iso`.
///
/// # Errors
///
/// [`ChecksumError::InvalidName`] for any other name.
pub fn plain(name: &str) -> Result<&str, ChecksumError> {
    match rc_rename::check_plain(name) {
        None => Ok(name),
        Some(_) => Err(ChecksumError::InvalidName {
            name: name.to_owned(),
        }),
    }
}

/// `name` when it is one entry of its folder a checksum line can carry: not empty, not `.` or
/// `..`, no separator (`/` or `\` on every backend), no drive colon, no control character. The
/// names given to hash and those a folder lists (a server's listing is untrusted) are such
/// names, and [`inside`] reads every one back, taking `\` as a separator everywhere.
///
/// # Errors
///
/// [`ChecksumError::InvalidName`] for any other name.
pub(crate) fn listed(name: &str) -> Result<&str, ChecksumError> {
    let bad = name.is_empty()
        || name == "."
        || name == ".."
        || name
            .chars()
            .any(|c| c == '/' || c == '\\' || c == ':' || c.is_control());
    if bad {
        Err(ChecksumError::InvalidName {
            name: name.to_owned(),
        })
    } else {
        Ok(name)
    }
}

/// `relative` - a `/`-separated name this crate built from listed entries - under `dir`, one
/// component at a time: a `\\?\` path takes no `/` as a separator.
pub(crate) fn join(dir: &Path, relative: &str) -> PathBuf {
    let mut path = dir.to_path_buf();
    path.extend(relative.split('/'));
    path
}

/// `name` from a checksum file, joined onto `base` - or why it may not be: it must stay inside
/// `base`. `/` and `\` both separate (Total Commander writes `\`), `.` parts and a leading `./`
/// are dropped; an absolute name, a drive, a UNC or device prefix, `..`, a colon (also an NTFS
/// stream) and control characters are refused.
pub(crate) fn inside(base: &Path, name: &str) -> Result<PathBuf, &'static str> {
    if name.is_empty() {
        return Err("empty_name");
    }
    if name.starts_with('/') || name.starts_with('\\') {
        return Err("absolute_name");
    }
    if name.chars().any(|c| c == ':' || c.is_control()) {
        return Err("forbidden_character");
    }
    let mut path = base.to_path_buf();
    let mut parts = 0_usize;
    for part in name.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return Err("parent_reference"),
            part => {
                path.push(part);
                parts += 1;
            }
        }
    }
    if parts == 0 {
        return Err("empty_name");
    }
    Ok(path)
}

#[cfg(test)]
#[path = "names_tests.rs"]
mod tests;
