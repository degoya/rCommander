//! The start's pass over the installed plugins: the staging folders of installs that stopped
//! half-way go, the versions this build can no longer load go where a loadable version of the
//! same plugin stands in for them, and every other version that does not verify is reported once.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/installed/sweep.rs`
//! (RD-1291-03) and `installed.rs` (`sweep_install_staging`, RD-170-07) at commit 26c8592a6, the
//! rules unchanged: an outdated version (an older manifest revision or contract than every one
//! this build links) is removed only while its plugin keeps a version that loads and the person
//! has not chosen it; a version refused for any other reason - a revoked key or digest, a file
//! changed on disk, a contract newer than this build's - stays, because deleting it would hide
//! exactly what someone has to look at.

use std::{collections::BTreeMap, path::Path};

use anyhow::Result;
use serde::Deserialize;

use super::{
    PluginState,
    disk::{read_optional_bounded, read_package_parts, version_directories},
    plugins_root,
};
use crate::{
    MANIFEST_VERSION, MAX_MANIFEST_BYTES, PluginInstaller, SUPPORTED_API_VERSIONS,
    package::{MANIFEST_MEMBER, VerifyError},
};

/// What the start's sweep did.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StartSweep {
    /// Staging folders of installs that stopped before their rename.
    pub stagings_removed: usize,
    /// `(plugin id, version)` of every outdated version removed.
    pub removed: Vec<(String, String)>,
    /// `(plugin id, version, code)` of every version kept although it does not verify.
    pub kept: Vec<(String, String, &'static str)>,
}

/// The fields that tell an outdated package from a broken one, in every manifest revision.
#[derive(Deserialize)]
struct ContractHeader {
    manifest_version: u32,
    api_version: Option<String>,
}

impl ContractHeader {
    /// An older manifest revision, or this one with an older `api_version` than every one this
    /// build links. A newer one waits for a newer build.
    fn outdated(&self) -> bool {
        if self.manifest_version != MANIFEST_VERSION {
            return self.manifest_version < MANIFEST_VERSION;
        }
        let Some(api) = self
            .api_version
            .as_deref()
            .and_then(|value| semver::Version::parse(value).ok())
        else {
            return false;
        };
        SUPPORTED_API_VERSIONS
            .iter()
            .filter_map(|value| semver::Version::parse(value).ok())
            .min()
            .is_some_and(|oldest| api < oldest)
    }
}

#[derive(Default)]
struct Versions {
    loadable: usize,
    /// `(version, folder, code, outdated)` of each one that does not verify.
    refused: Vec<(String, std::path::PathBuf, &'static str, bool)>,
}

/// Sweeps `<data dir>/plugins` under `state`. Never fails the start: what cannot be read is
/// reported and left.
#[must_use]
pub fn sweep_at_start(data_dir: &Path, state: &PluginState) -> StartSweep {
    let mut sweep = StartSweep::default();
    let verifier = match state.verifier() {
        Ok(verifier) => verifier,
        Err(error) => {
            tracing::warn!(%error, "the plugin state does not decode; installed plugins are not checked");
            return sweep;
        }
    };
    let installer = PluginInstaller::new(plugins_root(data_dir), verifier);
    sweep.stagings_removed = installer.sweep_install_staging();
    let plugins = match classify(data_dir, &installer) {
        Ok(plugins) => plugins,
        Err(error) => {
            tracing::warn!(%error, "installed plugin versions could not be checked");
            return sweep;
        }
    };
    for (id, versions) in plugins {
        let choice = state.choices.get(&id);
        for (version, folder, code, outdated) in versions.refused {
            let chosen = choice.is_some_and(|choice| {
                [&choice.active, &choice.staged]
                    .into_iter()
                    .any(|chosen| chosen.as_deref() == Some(version.as_str()))
            });
            if outdated && versions.loadable > 0 && !chosen {
                match std::fs::remove_dir_all(&folder) {
                    Ok(()) => {
                        sweep.removed.push((id.clone(), version));
                        continue;
                    }
                    Err(error) => {
                        tracing::warn!(%error, folder = %folder.display(), "an outdated plugin version could not be removed")
                    }
                }
            }
            tracing::warn!(
                plugin_id = %id,
                %version,
                code,
                "an installed plugin version does not verify and is kept"
            );
            sweep.kept.push((id.clone(), version, code));
        }
    }
    sweep
}

fn classify(data_dir: &Path, installer: &PluginInstaller) -> Result<BTreeMap<String, Versions>> {
    let mut plugins: BTreeMap<String, Versions> = BTreeMap::new();
    for folder in version_directories(&plugins_root(data_dir))? {
        let name = |path: Option<&Path>| {
            path.and_then(Path::file_name)
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let (id, version) = (name(folder.parent()), name(Some(&folder)));
        let plugin = plugins.entry(id).or_default();
        let checked = read_package_parts(&folder)
            .map_err(VerifyError::from)
            .and_then(|parts| installer.verifier().verify_contents(parts));
        match checked {
            Ok(_) => plugin.loadable += 1,
            Err(error) => {
                let outdated =
                    read_optional_bounded(&folder.join(MANIFEST_MEMBER), MAX_MANIFEST_BYTES as u64)
                        .ok()
                        .flatten()
                        .and_then(|bytes| String::from_utf8(bytes).ok())
                        .and_then(|text| toml::from_str::<ContractHeader>(&text).ok())
                        .is_some_and(|header| header.outdated());
                plugin
                    .refused
                    .push((version, folder.clone(), error.code(), outdated));
            }
        }
    }
    Ok(plugins)
}
