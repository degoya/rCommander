//! `PluginInstaller`: a verified package is written to a staging folder beside the version
//! folders and renamed into `<data dir>/plugins/<id>/<version>/`, so a stop half-way never leaves
//! a version that loads.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/install.rs` (`install_bytes`,
//! `install_verified`, `write_staging`, `remove_installed`) and `installed.rs` (`remove_version`,
//! `sweep_install_staging`, `installed_package_digest`) at commit 26c8592a6. Adapted: blocking
//! file calls (the engine runs them off the UI thread), the staging and the rename as two steps a
//! test can stop between, a version an open viewer holds (`installed::hold`) not removable, and
//! the marker of a bundled version.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::{
    PluginManifest, VerifiedPackage,
    installed::{
        disk::{
            BUNDLED_MARKER, INSTALL_STAGING_PREFIX, install_stagings, read_package_parts,
            safe_segment,
        },
        in_use,
    },
    package::{COMPONENT_MEMBER, MANIFEST_MEMBER, SIGNATURE_MEMBER, VerifyError},
    package_digest,
    verifier::{PluginVerifier, Refusal},
};

/// One installed version: where it landed and what it is.
#[derive(Debug)]
pub struct InstalledPackage {
    pub path: PathBuf,
    pub manifest: PluginManifest,
    /// [`crate::package_digest`] of what was installed.
    pub digest: [u8; 32],
}

/// Installs packages into `<root>/<plugin id>/<version>`, `root` being `<data dir>/plugins`.
#[derive(Clone, Debug)]
pub struct PluginInstaller {
    root: PathBuf,
    verifier: PluginVerifier,
}

impl PluginInstaller {
    #[must_use]
    pub fn new(root: PathBuf, verifier: PluginVerifier) -> Self {
        Self { root, verifier }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn verifier(&self) -> &PluginVerifier {
        &self.verifier
    }

    /// Verifies and installs a package file.
    ///
    /// # Errors
    ///
    /// [`VerifyError::UntrustedKey`] for a key nobody confirmed yet; otherwise the refusal or the
    /// failed write.
    pub fn install_file(&self, path: &Path) -> Result<InstalledPackage, VerifyError> {
        let package = self.verifier.verify_file(path)?;
        Ok(self.install_verified(&package, false)?)
    }

    /// Verifies and installs package bytes.
    ///
    /// # Errors
    ///
    /// As [`PluginInstaller::install_file`].
    pub fn install_bytes(&self, bytes: &[u8]) -> Result<InstalledPackage, VerifyError> {
        let package = self.verifier.verify_bytes(bytes)?;
        Ok(self.install_verified(&package, false)?)
    }

    /// Installs a verified package; an installed version is never replaced in place.
    ///
    /// # Errors
    ///
    /// The version is installed already, or a write or the rename fails - nothing is left then.
    pub fn install_verified(
        &self,
        package: &VerifiedPackage,
        bundled: bool,
    ) -> Result<InstalledPackage> {
        let destination = self.version_dir(&package.manifest.id, &package.manifest.version)?;
        if destination.exists() {
            return Err(Refusal {
                code: "plugin_version_installed",
                reason: format!(
                    "{} {} is installed already",
                    package.manifest.name, package.manifest.version
                ),
            }
            .into());
        }
        let staging = self.stage(package, bundled)?;
        Self::promote(&staging, &destination)?;
        Ok(InstalledPackage {
            path: destination,
            manifest: package.manifest.clone(),
            digest: package.digest(),
        })
    }

    /// Writes the package into a fresh staging folder beside its plugin's versions. A stop after
    /// this leaves only that folder: never loaded, removed by the next start.
    pub(crate) fn stage(&self, package: &VerifiedPackage, bundled: bool) -> Result<PathBuf> {
        let plugin_root = self.root.join(safe_segment(&package.manifest.id)?);
        std::fs::create_dir_all(&plugin_root)
            .with_context(|| format!("create {}", plugin_root.display()))?;
        let staging = plugin_root.join(format!(
            "{INSTALL_STAGING_PREFIX}{}",
            crate::sign::hex_sha256(&rand_bytes())
        ));
        std::fs::create_dir(&staging)?;
        if let Err(error) = write_staging(&staging, package, bundled) {
            remove_quietly(&staging);
            return Err(error);
        }
        Ok(staging)
    }

    /// The rename that makes a staged version an installed one; the staging folder goes when it
    /// fails.
    pub(crate) fn promote(staging: &Path, destination: &Path) -> Result<()> {
        if let Err(error) = std::fs::rename(staging, destination) {
            remove_quietly(staging);
            return Err(error).with_context(|| format!("install {}", destination.display()));
        }
        Ok(())
    }

    /// Removes one installed version; whether anything was there. A version an open viewer holds
    /// is refused (`plugin_version_in_use`).
    ///
    /// # Errors
    ///
    /// A segment is no plugin id or version, the version is in use, or the removal fails.
    pub fn remove_version(&self, id: &str, version: &str) -> Result<bool> {
        let path = self.version_dir(id, version)?;
        if !path.is_dir() {
            return Ok(false);
        }
        if in_use(&path) {
            return Err(Refusal {
                code: "plugin_version_in_use",
                reason: format!("version {version} is in use by an open viewer"),
            }
            .into());
        }
        std::fs::remove_dir_all(&path).with_context(|| format!("remove {}", path.display()))?;
        // The plugin's folder goes with its last version.
        if let Some(parent) = path.parent()
            && std::fs::read_dir(parent).is_ok_and(|mut entries| entries.next().is_none())
        {
            let _ = std::fs::remove_dir(parent);
        }
        Ok(true)
    }

    /// Removes what installs that stopped before their rename left; how many. Called once at
    /// start, before anything installs.
    #[must_use]
    pub fn sweep_install_staging(&self) -> usize {
        let stagings = match install_stagings(&self.root) {
            Ok(stagings) => stagings,
            Err(error) => {
                tracing::warn!(%error, "leftover plugin install folders could not be listed");
                return 0;
            }
        };
        let mut removed = 0;
        for staging in stagings {
            match std::fs::remove_dir_all(&staging) {
                Ok(()) => removed += 1,
                Err(error) => tracing::warn!(
                    %error,
                    path = %staging.display(),
                    "a leftover plugin install folder could not be removed"
                ),
            }
        }
        removed
    }

    /// The content digest of one installed version, hashed from the members on disk without a
    /// signature check: a package whose signature no longer holds is exactly one a person may
    /// want to withdraw. `None` when that version is not installed.
    ///
    /// # Errors
    ///
    /// A segment is no plugin id or version, or a member cannot be read.
    pub fn installed_package_digest(&self, id: &str, version: &str) -> Result<Option<[u8; 32]>> {
        let path = self.version_dir(id, version)?;
        if !path.is_dir() {
            return Ok(None);
        }
        let (manifest, component, _signature, locales) = read_package_parts(&path)?;
        Ok(Some(package_digest(&manifest, &component, &locales)))
    }

    /// `<root>/<id>/<version>`, both checked before they touch the path.
    ///
    /// # Errors
    ///
    /// A segment could leave the plugin folder.
    pub fn version_dir(&self, id: &str, version: &str) -> Result<PathBuf> {
        Ok(self
            .root
            .join(safe_segment(id)?)
            .join(safe_segment(version)?))
    }
}

fn write_staging(staging: &Path, package: &VerifiedPackage, bundled: bool) -> Result<()> {
    std::fs::write(staging.join(MANIFEST_MEMBER), &package.manifest_bytes)?;
    std::fs::write(staging.join(COMPONENT_MEMBER), &package.component)?;
    if let Some(signature) = &package.signature {
        std::fs::write(staging.join(SIGNATURE_MEMBER), signature)?;
    }
    if !package.locales.is_empty() {
        let directory = staging.join("locales");
        std::fs::create_dir(&directory)?;
        for (language, bytes) in &package.locales {
            std::fs::write(directory.join(format!("{language}.json")), bytes)?;
        }
    }
    if bundled {
        std::fs::write(staging.join(BUNDLED_MARKER), [])?;
    }
    Ok(())
}

fn remove_quietly(folder: &Path) {
    if let Err(error) = std::fs::remove_dir_all(folder) {
        tracing::debug!(%error, folder = %folder.display(), "a plugin staging folder could not be removed");
    }
}

fn rand_bytes() -> [u8; 16] {
    use rand::Rng as _;
    let mut bytes = [0_u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    bytes
}

#[cfg(test)]
#[path = "install_tests.rs"]
mod tests;
