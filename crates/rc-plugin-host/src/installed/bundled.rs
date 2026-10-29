//! The plugins shipped with rCommander: `.rcplug` files in `plugins/` beside the executable -
//! inside a macOS app bundle in `Contents/Resources/plugins/` (RC-100-44) - signed with a
//! compiled-in root and installed at the start like any other package.
//!
//! Adapted from rDownloader's `crates/rd-plugin-host/src/bundled.rs` (`sync_bundled`) at commit
//! 26c8592a6: a bundled version not installed yet is installed, one installed already is left
//! alone, so a person who switched it off keeps it off and a rollback keeps its target. Only a
//! root key vouches for a bundled package - a file another key signed is skipped, never a trust
//! question at the start - and it is switched off like every plugin (`docs/plugins.md`,
//! "Installation and lifecycle").

use std::path::{Path, PathBuf};

use anyhow::Result;

use super::disk::read_bounded_file;
use crate::{
    PluginInstaller, PluginVerifier,
    package::{MAX_PACKAGE_BYTES, PACKAGE_EXTENSION},
};

/// The folder beside the executable the bundled packages are in.
pub const BUNDLED_DIR: &str = "plugins";

/// `plugins/` beside the running executable, or in the app bundle's resources; `None` when the
/// executable's folder is unknown.
#[must_use]
pub fn bundled_dir() -> Option<PathBuf> {
    bundled_dir_of(&std::env::current_exe().ok()?)
}

/// The bundled packages' folder for the executable at `exe`: `<app>/Contents/Resources/plugins`
/// when it is `<app>/Contents/MacOS/<name>` - code signing keeps `MacOS/` for code - else
/// `plugins/` beside it. Read the same everywhere, so it is tested everywhere.
fn bundled_dir_of(exe: &Path) -> Option<PathBuf> {
    let folder = exe.parent()?;
    if folder.file_name().is_some_and(|name| name == "MacOS")
        && let Some(contents) = folder.parent()
        && contents.file_name().is_some_and(|name| name == "Contents")
    {
        return Some(contents.join("Resources").join(BUNDLED_DIR));
    }
    Some(folder.join(BUNDLED_DIR))
}

/// Installs every bundled package from `bundled` that is not installed yet into
/// `<data dir>/plugins`; the `(id, version)` of each one installed. A package that does not
/// verify against the roots is skipped with a warning; the start goes on.
///
/// # Errors
///
/// The roots do not decode.
pub fn sync_bundled(data_dir: &Path, bundled: &Path) -> Result<Vec<(String, String)>> {
    sync_with(data_dir, bundled, PluginVerifier::with_roots(false)?)
}

/// [`sync_bundled`] under `verifier`, so a test can stand in for the roots.
pub(crate) fn sync_with(
    data_dir: &Path,
    bundled: &Path,
    verifier: PluginVerifier,
) -> Result<Vec<(String, String)>> {
    let installer = PluginInstaller::new(super::plugins_root(data_dir), verifier);
    let mut installed = Vec::new();
    let Ok(entries) = std::fs::read_dir(bundled) else {
        return Ok(installed);
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension == PACKAGE_EXTENSION)
        })
        .collect();
    files.sort();
    for file in files {
        let verified = read_bounded_file(&file, MAX_PACKAGE_BYTES)
            .map_err(crate::VerifyError::from)
            .and_then(|bytes| installer.verifier().verify_bytes(&bytes));
        let package = match verified {
            Ok(package) => package,
            Err(error) => {
                tracing::warn!(
                    package = %file.display(),
                    code = error.code(),
                    "a bundled plugin package does not verify against rCommander's keys"
                );
                continue;
            }
        };
        let manifest = &package.manifest;
        if installer
            .version_dir(&manifest.id, &manifest.version)?
            .exists()
        {
            continue;
        }
        match installer.install_verified(&package, true) {
            Ok(done) => installed.push((done.manifest.id, done.manifest.version)),
            Err(error) => tracing::warn!(
                package = %file.display(),
                %error,
                "a bundled plugin package could not be installed"
            ),
        }
    }
    Ok(installed)
}

#[cfg(test)]
#[path = "bundled_tests.rs"]
mod tests;
