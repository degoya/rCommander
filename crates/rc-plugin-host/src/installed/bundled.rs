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
//! is replaced by the bundled package of the same id and version. So is one rebuilt (RC-110-54):
//! it verifies and the bundled package of its id and version is signed by the same key, but the
//! contents differ - a new build under an unchanged version, which would otherwise stay on the
//! old build and compile the new one at every start. Only that: a version under a key the person
//! trusts, one a person installed, one withdrawn or changed on disk stays as it is. The switch,
//! the version choice and the settings are kept by id and version, outside the folder.
//!
//! New in rCommander (RC-110-64): a package unchanged since the last start, under unchanged
//! trust, with its version still installed, is passed over unread (`seen.rs`).

mod seen;

use std::{
    collections::BTreeSet,
    io::Cursor,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};

use super::{
    PluginState,
    disk::{BUNDLED_MARKER, read_bounded_file, read_package_parts},
    in_use,
};
use crate::{
    PluginInstaller, PluginVerifier, VerifiedPackage,
    package::{MAX_PACKAGE_BYTES, PACKAGE_EXTENSION, read_archive},
};
use seen::{FileStamp, Seen};

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
/// longer trusts or rebuilt under the same key; the `(id, version)` of each one installed or
/// replaced. A package that does not
/// verify against the roots is skipped with a warning; the start goes on.
///
/// # Errors
///
/// The roots do not decode.
pub fn sync_bundled(data_dir: &Path, bundled: &Path) -> Result<Vec<(String, String)>> {
    sync_stamped(
        data_dir,
        bundled,
        || PluginVerifier::with_roots(false),
        &seen::roots_stamp(),
    )
}

/// [`sync_bundled`] under the verifiers `roots` makes, so a test can stand in for the roots.
#[cfg(test)]
pub(crate) fn sync_with(
    data_dir: &Path,
    bundled: &Path,
    roots: impl Fn() -> Result<PluginVerifier>,
) -> Result<Vec<(String, String)>> {
    sync_stamped(data_dir, bundled, roots, "test roots")
}

/// [`sync_bundled`] under the verifiers `roots` makes, which `roots_stamp` names for the record
/// of the packages judged already.
fn sync_stamped(
    data_dir: &Path,
    bundled: &Path,
    roots: impl Fn() -> Result<PluginVerifier>,
    roots_stamp: &str,
) -> Result<Vec<(String, String)>> {
    let installer = PluginInstaller::new(super::plugins_root(data_dir), roots()?);
    let mut installed = Vec::new();
    let Ok(entries) = std::fs::read_dir(bundled) else {
        return Ok(installed);
    };
    // The installed versions are judged as `list` runs them: the roots with the person's keys and
    // withdrawals. A state that does not read replaces nothing, and keeps no record.
    let judged = PluginState::read(data_dir).and_then(|state| {
        let verifier = state.verifier_on(roots()?)?;
        Ok((verifier, seen::context(roots_stamp, &state)))
    });
    let (current, mut seen, last) = match judged {
        Ok((verifier, context)) => (
            Some(verifier),
            Some(Seen::new(context.clone())),
            Seen::read(data_dir, &context),
        ),
        Err(error) => {
            tracing::warn!(%error, "the plugin state does not decode; no bundled plugin is replaced");
            (None, None, Seen::default())
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
        let stamp = FileStamp::of(&file);
        if let Some(judged) = last.unchanged(&file, stamp, &installer) {
            if let Some(seen) = seen.as_mut() {
                seen.keep(&file, judged.clone());
            }
            continue;
        }
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
            let Some(reason) = current
                .as_ref()
                .and_then(|current| replacement(&destination, &package, current))
            else {
                if let Some(seen) = seen.as_mut() {
                    seen.record(&file, stamp, &package);
                }
                continue;
            };
            match replace(&installer, &package, &destination) {
                Ok(()) => {
                    match reason {
                        Replacement::NoLongerTrusted(code) => tracing::warn!(
                            plugin_id = %manifest.id,
                            version = %manifest.version,
                            code,
                            "an installed bundled plugin no longer verified and was replaced by \
                             the bundled package"
                        ),
                        Replacement::Rebuilt => tracing::info!(
                            plugin_id = %manifest.id,
                            version = %manifest.version,
                            "an installed bundled plugin was replaced by its rebuilt bundled \
                             package"
                        ),
                    }
                    installed.push((manifest.id.clone(), manifest.version.clone()));
                    if let Some(seen) = seen.as_mut() {
                        seen.record(&file, stamp, &package);
                    }
                }
                Err(error) => tracing::warn!(
                    plugin_id = %manifest.id,
                    version = %manifest.version,
                    reason = ?reason,
                    %error,
                    "an installed bundled plugin could not be replaced by the bundled package"
                ),
            }
            continue;
        }
        match installer.install_verified(&package, true) {
            Ok(done) => {
                installed.push((done.manifest.id, done.manifest.version));
                if let Some(seen) = seen.as_mut() {
                    seen.record(&file, stamp, &package);
                }
            }
            Err(error) => tracing::warn!(
                package = %file.display(),
                %error,
                "a bundled plugin package could not be installed"
            ),
        }
    }
    // A record that is not written costs the next start a full check, nothing else.
    if let Some(seen) = seen
        && let Err(error) = seen.write(data_dir)
    {
        tracing::warn!(%error, "the record of the bundled plugins could not be written");
    }
    Ok(installed)
}

/// The `(id, version)` of every package in `bundled`, as its manifest names it: what the start's
/// [`sync_bundled`] installs again when it is missing, so "Remove old versions..." leaves it
/// (RC-110-55). Read, not verified - a claim can only keep a version, never remove one; a file
/// that does not read is passed over.
#[must_use]
pub fn bundled_versions(bundled: &Path) -> BTreeSet<(String, String)> {
    let Ok(entries) = std::fs::read_dir(bundled) else {
        return BTreeSet::new();
    };
    entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension == PACKAGE_EXTENSION)
        })
        .filter_map(|path| {
            let bytes = read_bounded_file(&path, MAX_PACKAGE_BYTES).ok()?;
            let (manifest, ..) = read_archive(Cursor::new(bytes)).ok()?;
            let manifest = crate::parse_manifest(&manifest).ok()?;
            Some((manifest.id, manifest.version))
        })
        .collect()
}

/// Why an installed bundled version gives way to the bundled package of its id and version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Replacement {
    /// Signed by a key this build does not trust (any more): the refusal's code (RC-100-79).
    NoLongerTrusted(&'static str),
    /// The same version rebuilt under the same key - other bytes (RC-110-54). Left installed,
    /// it would keep the old build and compile the new one at every start for nothing.
    Rebuilt,
}

/// Whether the installed version in `dir` gives way to `package`: when it is a bundled one and
/// either signed by a key `current` does not trust, or verified, signed by `package`'s key and
/// of other contents. `None` for any other version - one a person installed, one under a key
/// the person trusts, one withdrawn or changed on disk.
fn replacement(
    dir: &Path,
    package: &VerifiedPackage,
    current: &PluginVerifier,
) -> Option<Replacement> {
    if !dir.join(BUNDLED_MARKER).is_file() {
        return None;
    }
    let verified = read_package_parts(dir)
        .map_err(crate::VerifyError::from)
        .and_then(|parts| current.verify_contents(parts));
    match verified {
        Ok(installed) => (installed.manifest.public_key == package.manifest.public_key
            && installed.digest() != package.digest())
        .then_some(Replacement::Rebuilt),
        Err(error) => {
            let code = error.code();
            NO_LONGER_TRUSTED
                .contains(&code)
                .then_some(Replacement::NoLongerTrusted(code))
        }
    }
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
