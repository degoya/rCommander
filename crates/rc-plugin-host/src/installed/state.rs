//! What decides which installed plugin runs, as one file beside the packages:
//! `<data dir>/plugins/state.json`.
//!
//! New in rCommander. The record is the database (`plugins`, `plugin_keys`,
//! `plugin_digest_revocations` and the `plugins.*` settings); the engine writes this file from it
//! at every start and after every change, so [`super::list`] - which RC-100-13's viewer adapter
//! calls with nothing but the data folder - verifies against the same keys, withdrawals, switches
//! and version choices without a database of its own. A missing file trusts the compiled-in roots
//! only and runs every plugin; an unreadable one runs none (`list` fails), since a switch a
//! person turned off must never be lost to a broken file. Whoever can write the data folder can
//! write the database too (`docs/security/plugins.md`, "Local administrator").

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write as _,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::disk::read_optional_bounded;
use crate::{PluginVerifier, VersionChoices, parse_package_digest};

/// The folder under the data folder the packages are installed into.
pub const PLUGINS_DIR: &str = "plugins";
/// The state file in it.
pub const STATE_FILE: &str = "state.json";
/// The largest state file read.
const MAX_STATE_BYTES: u64 = 4 * 1024 * 1024;

/// The installed plugins' switches, trust and version choices.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PluginState {
    /// `plugins.enabled`: off, no plugin runs.
    pub plugins_enabled: bool,
    /// `plugins.development_mode`: unsigned packages are accepted, marked as such.
    pub development_mode: bool,
    /// The plugin ids a person switched off.
    pub disabled: BTreeSet<String>,
    /// Keys a person confirmed on first use: key id to base64 public key.
    pub trusted_keys: BTreeMap<String, String>,
    /// Revoked signing keys, by fingerprint.
    pub revoked_keys: BTreeSet<String>,
    /// Withdrawn packages, by digest in hex.
    pub revoked_digests: BTreeSet<String>,
    /// The active and staged version of each plugin that has a choice.
    pub choices: VersionChoices,
}

impl Default for PluginState {
    fn default() -> Self {
        Self {
            plugins_enabled: true,
            development_mode: false,
            disabled: BTreeSet::new(),
            trusted_keys: BTreeMap::new(),
            revoked_keys: BTreeSet::new(),
            revoked_digests: BTreeSet::new(),
            choices: VersionChoices::new(),
        }
    }
}

/// `<data dir>/plugins`.
#[must_use]
pub fn plugins_root(data_dir: &Path) -> PathBuf {
    data_dir.join(PLUGINS_DIR)
}

impl PluginState {
    /// The state in `data_dir`; the default when none was written yet.
    ///
    /// # Errors
    ///
    /// The file cannot be read or does not parse.
    pub fn read(data_dir: &Path) -> Result<Self> {
        let path = plugins_root(data_dir).join(STATE_FILE);
        match read_optional_bounded(&path, MAX_STATE_BYTES)? {
            None => Ok(Self::default()),
            Some(bytes) => {
                serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
            }
        }
    }

    /// Writes the state into `data_dir`, whole: a temporary file renamed over the old one.
    ///
    /// # Errors
    ///
    /// The folder or the file cannot be written.
    pub fn write(&self, data_dir: &Path) -> Result<()> {
        let root = plugins_root(data_dir);
        std::fs::create_dir_all(&root).with_context(|| format!("create {}", root.display()))?;
        let bytes = serde_json::to_vec_pretty(self)?;
        let temporary = root.join(format!(".{STATE_FILE}.tmp"));
        let mut file = std::fs::File::create(&temporary)
            .with_context(|| format!("write {}", temporary.display()))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, root.join(STATE_FILE))
            .with_context(|| format!("replace {}", root.join(STATE_FILE).display()))
    }

    /// A verifier for this state: the compiled-in roots, the confirmed keys, the withdrawals.
    ///
    /// # Errors
    ///
    /// A stored key or digest does not decode: the state is broken, and nothing is trusted on it.
    pub fn verifier(&self) -> Result<PluginVerifier> {
        let verifier = PluginVerifier::with_roots(self.development_mode)?;
        for (key_id, public_key) in &self.trusted_keys {
            // A confirmed key never replaces a root of the same id.
            if verifier.trusted_key(key_id)?.is_some() {
                bail!("the confirmed key {key_id} names a compiled-in root");
            }
            verifier.trust_key_base64(key_id.clone(), public_key)?;
        }
        verifier
            .revoked_keys()
            .replace(self.revoked_keys.iter().cloned())?;
        let digests = self
            .revoked_digests
            .iter()
            .map(|digest| parse_package_digest(digest))
            .collect::<Result<Vec<_>>>()?;
        verifier.revoked_digests().replace(digests)?;
        Ok(verifier)
    }

    /// Whether the plugin `id` may run: the master switch on and the plugin not switched off.
    #[must_use]
    pub fn enabled(&self, id: &str) -> bool {
        self.plugins_enabled && !self.disabled.contains(id)
    }
}
