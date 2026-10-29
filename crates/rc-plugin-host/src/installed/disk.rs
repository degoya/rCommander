//! The installed packages on disk: the version folders, the staging folders an install left, and
//! reading one version's members with a size bound each.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/installed/disk.rs`
//! (`version_directories`, `install_stagings`, `read_package_parts`, `read_bounded_file`) at
//! commit 26c8592a6.

use std::{
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};

use crate::{
    MAX_LOCALE_BYTES, MAX_MANIFEST_BYTES,
    package::{
        COMPONENT_MEMBER, MANIFEST_MEMBER, MAX_COMPONENT_BYTES, MAX_SIGNATURE_BYTES, PackageParts,
        SIGNATURE_MEMBER,
    },
    valid_language,
};

/// What an install's staging folder beside the version folders is named after.
pub(crate) const INSTALL_STAGING_PREFIX: &str = ".install-";
/// The empty file that marks a version as one of the bundled packages beside the executable.
pub(crate) const BUNDLED_MARKER: &str = "bundled";

/// A plugin id or version as a folder name: nothing a path could make more of.
pub(crate) fn safe_segment(segment: &str) -> Result<&str> {
    let plain = !segment.is_empty()
        && segment.len() <= 128
        && segment != "."
        && segment != ".."
        && !segment.starts_with('.')
        && segment
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'+' | b'_'));
    if !plain {
        bail!("`{segment}` cannot name a plugin folder");
    }
    Ok(segment)
}

/// Every `<root>/<plugin id>/<version>` folder, without stray files and the staging folders of
/// installs that stopped before their rename: loading one would list a second copy of a plugin.
pub(crate) fn version_directories(root: &Path) -> Result<Vec<PathBuf>> {
    let mut directories = Vec::new();
    for plugin in plugin_directories(root)? {
        for version in std::fs::read_dir(&plugin)? {
            let version = version?;
            if version.file_type()?.is_dir() && !is_install_staging(&version.file_name()) {
                directories.push(version.path());
            }
        }
    }
    directories.sort();
    Ok(directories)
}

/// Every staging folder an install left under `root` without renaming it into place.
pub(crate) fn install_stagings(root: &Path) -> Result<Vec<PathBuf>> {
    let mut stagings = Vec::new();
    for plugin in plugin_directories(root)? {
        for entry in std::fs::read_dir(&plugin)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() && is_install_staging(&entry.file_name()) {
                stagings.push(entry.path());
            }
        }
    }
    Ok(stagings)
}

fn plugin_directories(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut plugins = Vec::new();
    for plugin in std::fs::read_dir(root)? {
        let plugin = plugin?;
        if plugin.file_type()?.is_dir() {
            plugins.push(plugin.path());
        }
    }
    Ok(plugins)
}

fn is_install_staging(name: &std::ffi::OsStr) -> bool {
    name.to_str()
        .is_some_and(|name| name.starts_with(INSTALL_STAGING_PREFIX))
}

/// The members of one installed version, each within its limit.
pub(crate) fn read_package_parts(version: &Path) -> Result<PackageParts> {
    Ok((
        read_bounded_file(&version.join(MANIFEST_MEMBER), MAX_MANIFEST_BYTES as u64)?,
        read_bounded_file(&version.join(COMPONENT_MEMBER), MAX_COMPONENT_BYTES)?,
        read_optional_bounded(&version.join(SIGNATURE_MEMBER), MAX_SIGNATURE_BYTES)?,
        read_locales(&version.join("locales"))?,
    ))
}

fn read_locales(directory: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut locales = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let language = name.strip_suffix(".json").filter(|tag| {
            valid_language(tag) && entry.file_type().is_ok_and(|kind| kind.is_file())
        });
        let Some(language) = language else {
            bail!("unexpected file {name} in installed locales directory");
        };
        locales.push((
            language.to_owned(),
            read_bounded_file(&entry.path(), MAX_LOCALE_BYTES)?,
        ));
    }
    locales.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(locales)
}

/// `None` when the file does not exist.
pub(crate) fn read_optional_bounded(path: &Path, limit: u64) -> Result<Option<Vec<u8>>> {
    match read_bounded_file(path, limit) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound) =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

pub(crate) fn read_bounded_file(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > limit {
        bail!("plugin file {} exceeds its size limit", path.display());
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    std::fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        bail!("plugin file {} exceeds its size limit", path.display());
    }
    Ok(bytes)
}
