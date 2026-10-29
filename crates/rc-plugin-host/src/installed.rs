//! The installed plugins: verified again on every listing, one running version each, and the
//! versions an open viewer holds.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/installed.rs` (`load_verified`,
//! `verified_contents`, the role of each version) and `installed/disk.rs` at commit 26c8592a6.
//! Adapted: [`list`] is a free function of the data folder - the seam RC-100-13's viewer adapter
//! calls - that reads the switches, keys, withdrawals and version choices from the engine's
//! [`PluginState`] instead of an installer held by a service; [`inventory`] is what the Plugins
//! page lists, every version with its standing; [`hold`] marks a version an open viewer uses.
//!
//! Layout: `<data dir>/plugins/<plugin id>/<version>/` holds `manifest.toml`, `component.wasm`,
//! `signature.ed25519` (unless unsigned in development mode), `locales/<lang>.json` and, for a
//! bundled package, the empty file `bundled`; `<data dir>/plugins/state.json` the state.

pub(crate) mod disk;

mod bundled;
mod keys;
mod state;
mod sweep;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};

use anyhow::Result;

use crate::{
    PluginManifest, VersionRole, format_package_digest, key_fingerprint,
    package::{VerifiedPackage, VerifyError},
    roles_by_id,
    sign::decode_public_key,
    verifier::PluginVerifier,
};
use disk::{BUNDLED_MARKER, read_package_parts, version_directories};

pub use bundled::{BUNDLED_DIR, bundled_dir, sync_bundled};
pub use keys::InstalledKeys;
pub use state::{PLUGINS_DIR, PluginState, STATE_FILE, plugins_root};
pub use sweep::{StartSweep, sweep_at_start};

/// One plugin that may be run: the version that runs, verified again just now.
pub struct InstalledPlugin {
    pub manifest: PluginManifest,
    /// The component's bytes, as verified.
    pub component: Vec<u8>,
    /// The master switch on and the plugin not switched off by the person.
    pub enabled: bool,
    /// One of the packages beside the executable.
    pub bundled: bool,
    /// `<data dir>/plugins/<id>/<version>`: what [`hold`] takes while a viewer uses it.
    pub version_dir: PathBuf,
    /// `false` only for a package development mode accepted unsigned; shown as such.
    pub signed: bool,
}

impl std::fmt::Debug for InstalledPlugin {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InstalledPlugin")
            .field("id", &self.manifest.id)
            .field("version", &self.manifest.version)
            .field("enabled", &self.enabled)
            .field("bundled", &self.bundled)
            .field("signed", &self.signed)
            .field("version_dir", &self.version_dir)
            .finish_non_exhaustive()
    }
}

/// Every installed plugin with the version that runs, verified in full - signature, withdrawals,
/// manifest, locales and the compile with its import check - against the state the engine wrote
/// into `data_dir`. A version that does not verify is left out; a plugin switched off is listed
/// with `enabled` off, so its caller can tell "off" from "not installed". Blocking: compiles a
/// component the process has not compiled yet.
///
/// # Errors
///
/// The plugin folder cannot be read, or the state is unreadable (then nothing runs).
pub fn list(data_dir: &Path) -> Result<Vec<InstalledPlugin>> {
    let state = PluginState::read(data_dir)?;
    let verifier = state.verifier()?;
    let mut loaded: Vec<(PathBuf, VerifiedPackage)> = Vec::new();
    for dir in version_directories(&plugins_root(data_dir))? {
        let verified = read_package_parts(&dir)
            .map_err(VerifyError::from)
            .and_then(|parts| verifier.verify_parts(parts))
            .and_then(|package| in_its_folder(&dir, package).map_err(VerifyError::from));
        match verified {
            Ok(package) => loaded.push((dir, package)),
            Err(error) => tracing::debug!(
                folder = %dir.display(),
                code = error.code(),
                "an installed plugin version does not verify and is left out"
            ),
        }
    }
    let roles = roles_of(loaded.iter().map(|(_, package)| &package.manifest), &state);
    let mut plugins: Vec<InstalledPlugin> = loaded
        .into_iter()
        .filter(|(_, package)| roles.get(&key(&package.manifest)) == Some(&VersionRole::Default))
        .map(|(dir, package)| InstalledPlugin {
            enabled: state.enabled(&package.manifest.id),
            bundled: dir.join(BUNDLED_MARKER).is_file(),
            signed: package.signature.is_some(),
            version_dir: dir,
            component: package.component,
            manifest: package.manifest,
        })
        .collect();
    plugins.sort_by(|left, right| {
        left.manifest
            .name
            .to_lowercase()
            .cmp(&right.manifest.name.to_lowercase())
    });
    Ok(plugins)
}

/// One installed version as the Plugins page shows it.
#[derive(Clone, Debug)]
pub struct VersionEntry {
    /// The plugin id and the version, as the folders name them.
    pub id: String,
    pub version: String,
    pub version_dir: PathBuf,
    pub bundled: bool,
    /// An open viewer holds it: not removable.
    pub in_use: bool,
    /// The manifest, when the version verifies.
    pub manifest: Option<PluginManifest>,
    /// Why it does not, when it does not: a [`crate::Refusal`] code, or `plugin_key_untrusted`.
    pub refused: Option<&'static str>,
    /// Where it stands among the plugin's versions that verify.
    pub role: Option<VersionRole>,
    pub signed: bool,
    /// The signing key's fingerprint.
    pub fingerprint: Option<String>,
    /// [`crate::package_digest`] in hex.
    pub digest: Option<String>,
}

/// Every installed version with its standing under `state`, by plugin id and newest first. Checks
/// the signatures, not the components: nothing is compiled for a listing, and a component that
/// does not compile is refused when something runs it ([`list`]).
///
/// # Errors
///
/// The plugin folder cannot be read, or the state's keys do not decode.
pub fn inventory(data_dir: &Path, state: &PluginState) -> Result<Vec<VersionEntry>> {
    let verifier = state.verifier()?;
    let mut entries: Vec<VersionEntry> = version_directories(&plugins_root(data_dir))?
        .into_iter()
        .map(|dir| entry(dir, &verifier))
        .collect();
    let roles = roles_of(
        entries.iter().filter_map(|entry| entry.manifest.as_ref()),
        state,
    );
    for entry in &mut entries {
        entry.role = entry
            .manifest
            .as_ref()
            .and_then(|manifest| roles.get(&key(manifest)).copied());
    }
    entries.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| crate::compare_versions(&right.version, &left.version))
    });
    Ok(entries)
}

fn entry(dir: PathBuf, verifier: &PluginVerifier) -> VersionEntry {
    let name = |path: Option<&Path>| {
        path.and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let parts = read_package_parts(&dir);
    let digest = parts
        .as_ref()
        .ok()
        .map(|(manifest, component, _, locales)| {
            format_package_digest(&crate::package_digest(manifest, component, locales))
        });
    let checked = parts
        .map_err(VerifyError::from)
        .and_then(|parts| verifier.verify_contents(parts))
        .and_then(|package| in_its_folder(&dir, package).map_err(VerifyError::from));
    let (manifest, refused, signed) = match checked {
        Ok(package) => {
            let signed = package.signature.is_some();
            (Some(package.manifest), None, signed)
        }
        Err(error) => (None, Some(error.code()), false),
    };
    let fingerprint = manifest
        .as_ref()
        .filter(|_| signed)
        .and_then(|manifest| decode_public_key(&manifest.public_key).ok())
        .map(|key| key_fingerprint(&key));
    VersionEntry {
        id: name(dir.parent()),
        version: name(Some(&dir)),
        bundled: dir.join(BUNDLED_MARKER).is_file(),
        in_use: in_use(&dir),
        version_dir: dir,
        manifest,
        refused,
        role: None,
        signed,
        fingerprint,
        digest,
    }
}

/// A package must lie in the folder of its own id and version: a copy under another name would
/// list as a version that cannot be removed or chosen by what it says it is.
fn in_its_folder(dir: &Path, package: VerifiedPackage) -> Result<VerifiedPackage> {
    let expected = Path::new(&package.manifest.id).join(&package.manifest.version);
    if dir.ends_with(&expected) {
        Ok(package)
    } else {
        Err(crate::Refusal {
            code: crate::codes::PLUGIN_INVALID,
            reason: format!(
                "{} does not hold the version its manifest names",
                dir.display()
            ),
        }
        .into())
    }
}

fn key(manifest: &PluginManifest) -> (String, String) {
    (manifest.id.clone(), manifest.version.clone())
}

fn roles_of<'a>(
    manifests: impl Iterator<Item = &'a PluginManifest>,
    state: &PluginState,
) -> std::collections::HashMap<(String, String), VersionRole> {
    let pairs: Vec<(String, String)> = manifests.map(key).collect();
    roles_by_id(&pairs, &state.choices)
}

/// The version folders open viewers hold, with how many each.
static IN_USE: Mutex<BTreeMap<PathBuf, usize>> = Mutex::new(BTreeMap::new());

/// A version held by an open viewer until this is dropped; while held it cannot be removed.
#[derive(Debug)]
pub struct InUse {
    version_dir: PathBuf,
}

/// Holds `version_dir` (an [`InstalledPlugin::version_dir`]) for as long as the guard lives.
#[must_use]
pub fn hold(version_dir: &Path) -> InUse {
    let mut held = IN_USE.lock().unwrap_or_else(PoisonError::into_inner);
    *held.entry(version_dir.to_path_buf()).or_default() += 1;
    InUse {
        version_dir: version_dir.to_path_buf(),
    }
}

/// Whether an open viewer holds `version_dir`.
#[must_use]
pub fn in_use(version_dir: &Path) -> bool {
    IN_USE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .contains_key(version_dir)
}

impl Drop for InUse {
    fn drop(&mut self) {
        let mut held = IN_USE.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(count) = held.get_mut(&self.version_dir) {
            *count -= 1;
            if *count == 0 {
                held.remove(&self.version_dir);
            }
        }
    }
}

#[cfg(test)]
#[path = "installed_tests.rs"]
mod tests;
