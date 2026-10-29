//! Subfolders, as in Total Commander: a `\` (or `/`) in a new name moves the entry into a
//! subfolder of its directory, which the run creates when it is missing (RC-020-02).
//!
//! A new name is untrusted input like the rest of a rule, and its target stays below the
//! directory: a path that starts at a root (`\x`), names a share (`\\server\x`) or a drive
//! (`C:\x`), climbs out (`..\x`) or has an empty part (`a\\b`, `a\`) is a conflict with its
//! reason, never a rename. Each part is checked as a name of its own ([`check_name`]), the whole
//! path against the longest path.
//!
//! A plan, its steps, the undo log and `rc-ops` spell a nested name with
//! [`SUBFOLDER_SEPARATOR`] only: no file system allows `/` in a name, so splitting at it is never
//! ambiguous, where `\` is an ordinary character on Linux.

use std::path::{Path, PathBuf};

use crate::names::{FsRules, InvalidName, check_name, path_too_long};

/// How a nested name is spelled in a plan, its steps and the undo log: `sub/name`.
pub const SUBFOLDER_SEPARATOR: char = '/';

/// `name` with each `\` spelled as [`SUBFOLDER_SEPARATOR`].
pub(crate) fn normalized(name: &str) -> String {
    name.replace('\\', "/")
}

/// Whether the new name `path` - one name, or a nested one spelled with
/// [`SUBFOLDER_SEPARATOR`] - can be created below `dir`; the first reason it cannot.
#[must_use]
pub fn check_path(path: &str, dir: &str, rules: &FsRules) -> Option<InvalidName> {
    if !path.contains(SUBFOLDER_SEPARATOR) {
        return check_name(path, dir, rules);
    }
    if path.starts_with("//") {
        return Some(InvalidName::UncPath);
    }
    if path.starts_with(SUBFOLDER_SEPARATOR) {
        return Some(InvalidName::AbsolutePath);
    }
    let mut chars = path.chars();
    if let (Some(letter), Some(':')) = (chars.next(), chars.next())
        && letter.is_ascii_alphabetic()
    {
        return Some(InvalidName::DrivePath);
    }
    let parts: Vec<&str> = path.split(SUBFOLDER_SEPARATOR).collect();
    if parts.iter().any(|part| part.is_empty()) {
        return Some(InvalidName::EmptyPart);
    }
    if parts.contains(&"..") {
        return Some(InvalidName::ParentDir);
    }
    if let Some(reason) = parts.iter().find_map(|part| check_name(part, dir, rules)) {
        return Some(reason);
    }
    path_too_long(path, dir, rules).then_some(InvalidName::PathTooLong)
}

/// The path of `name` - one name, or a nested one spelled with [`SUBFOLDER_SEPARATOR`] - below
/// `dir`, joined part by part, so it holds for every backend's separator and for Windows' long
/// paths (`\\?\`).
#[must_use]
pub fn nested_path(dir: &Path, name: &str) -> PathBuf {
    name.split(SUBFOLDER_SEPARATOR)
        .fold(dir.to_path_buf(), |path, part| path.join(part))
}

#[cfg(test)]
#[path = "subdir_tests.rs"]
mod tests;
