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
//!
//! New in rCommander (RC-100-79): an installed bundled version signed by a key this build no
//! longer trusts - the development key after the release key, or a release key after its change -
//! is replaced by the bundled package of the same id and version. Only that: a version that
//! verifies, one a person installed, one withdrawn or changed on disk stays as it is. The switch,
//! the version choice and the settings are kept by id and version, outside the folder.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

use super::{
    PluginState,
    disk::{BUNDLED_MARKER, read_bounded_file, read_package_parts},
    in_use,
};
use crate::{
    PluginInstaller, PluginVerifier, VerifiedPackage,
    package::{MAX_PACKAGE_BYTES, PACKAGE_EXTENSION},
};

/// The refusals of an installed bundled version that its bundled package replaces: signed, but by
/// a key this build does not trust (any more) under that key id.
const NO_LONGER_TRUSTED: [&str; 2] = ["plugin_key_untrusted", "plugin_key_mismatch"];

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
/// `<data dir>/plugins`, and replaces an installed bundled version signed by a key this build no
/// longer trusts; the `(id, version)` of each one installed or replaced. A package that does not
/// verify against the roots is skipped with a warning; the start goes on.
///
/// # Errors
///
/// The roots do not decode.
pub fn sync_bundled(data_dir: &Path, bundled: &Path) -> Result<Vec<(String, String)>> {
    sync_with(data_dir, bundled, || PluginVerifier::with_roots(false))
}

/// [`sync_bundled`] under the verifiers `roots` makes, so a test can stand in for the roots.
pub(crate) fn sync_with(
    data_dir: &Path,
    bundled: &Path,
    roots: impl Fn() -> Result<PluginVerifier>,
) -> Result<Vec<(String, String)>> {
    let installer = PluginInstaller::new(super::plugins_root(data_dir), roots()?);
    let mut installed = Vec::new();
    let Ok(entries) = std::fs::read_dir(bundled) else {
        return Ok(installed);
    };
    // The installed versions are judged as `list` runs them: the roots with the person's keys and
    // withdrawals. A state that does not read replaces nothing.
    let current = match PluginState::read(data_dir).and_then(|state| state.verifier_on(roots()?)) {
        Ok(verifier) => Some(verifier),
        Err(error) => {
            tracing::warn!(%error, "the plugin state does not decode; no bundled plugin is replaced");
            None
        }
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
        let destination = installer.version_dir(&manifest.id, &manifest.version)?;
        if destination.exists() {
            let Some(code) = current
                .as_ref()
                .and_then(|current| no_longer_trusted(&destination, current))
            else {
                continue;
            };
            match replace(&installer, &package, &destination) {
                Ok(()) => {
                    tracing::warn!(
                        plugin_id = %manifest.id,
                        version = %manifest.version,
                        code,
                        "an installed bundled plugin no longer verified and was replaced by the \
                         bundled package"
                    );
                    installed.push((manifest.id.clone(), manifest.version.clone()));
                }
                Err(error) => tracing::warn!(
                    plugin_id = %manifest.id,
                    version = %manifest.version,
                    code,
                    %error,
                    "an installed bundled plugin that no longer verifies could not be replaced"
                ),
            }
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

/// The refusal of the installed version in `dir` when it is a bundled one signed by a key
/// `current` does not trust; `None` for any other version, verified or not.
fn no_longer_trusted(dir: &Path, current: &PluginVerifier) -> Option<&'static str> {
    if !dir.join(BUNDLED_MARKER).is_file() {
        return None;
    }
    let code = read_package_parts(dir)
        .map_err(crate::VerifyError::from)
        .and_then(|parts| current.verify_contents(parts))
        .err()?
        .code();
    NO_LONGER_TRUSTED.contains(&code).then_some(code)
}

/// Puts `package` in place of the version in `destination`: staged first, the old version moved
/// aside under a staging name, the new one renamed in, the old one removed. A stop half-way
/// leaves staging folders the start's sweep removes, and the next start installs the version.
fn replace(
    installer: &PluginInstaller,
    package: &VerifiedPackage,
    destination: &Path,
) -> Result<()> {
    if in_use(destination) {
        bail!("{} is in use by an open viewer", destination.display());
    }
    let staging = installer.stage(package, true)?;
    let mut aside = staging.clone().into_os_string();
    aside.push("-replaced");
    let aside = PathBuf::from(aside);
    if let Err(error) = std::fs::rename(destination, &aside) {
        remove_quietly(&staging);
        return Err(error).with_context(|| format!("move {} aside", destination.display()));
    }
    if let Err(error) = PluginInstaller::promote(&staging, destination) {
        // The old version back: no worse than before.
        if let Err(back) = std::fs::rename(&aside, destination) {
            tracing::warn!(error = %back, folder = %destination.display(), "a replaced plugin version could not be put back");
        }
        return Err(error);
    }
    remove_quietly(&aside);
    Ok(())
}

fn remove_quietly(folder: &Path) {
    if let Err(error) = std::fs::remove_dir_all(folder) {
        tracing::debug!(%error, folder = %folder.display(), "a plugin staging folder could not be removed");
    }
}

#[cfg(test)]
#[path = "bundled_tests.rs"]
mod tests;
