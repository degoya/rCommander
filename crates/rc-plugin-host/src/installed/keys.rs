//! The signing key each installed plugin was published under (RC-080-43). An update - the same
//! plugin id - under another key is refused (`plugin_key_changed`) until the person removes the
//! installed plugin: a stranger's key never inherits its settings, associations and granted
//! port, whatever the key's own standing.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};

use anyhow::Result;

use super::{
    disk::{read_bounded_file, version_directories},
    state::plugins_root,
};
use crate::{
    MAX_MANIFEST_BYTES,
    package::MANIFEST_MEMBER,
    parse_manifest,
    sign::{decode_public_key, key_fingerprint},
};

/// What a version whose manifest or key does not read is recorded under: no key matches it.
const UNREADABLE: &str = "";

/// The fingerprints of the keys the installed versions of each plugin name, by plugin id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InstalledKeys(Arc<BTreeMap<String, BTreeSet<String>>>);

impl InstalledKeys {
    /// The keys of every version under `data_dir`'s plugin folder, each under the folder it was
    /// installed into (named after its plugin id). A version whose manifest or key does not read
    /// is recorded as matching no key, so its plugin takes no update before it is removed.
    ///
    /// # Errors
    ///
    /// The plugin folder cannot be listed.
    pub fn read(data_dir: &Path) -> Result<Self> {
        let mut entries = Vec::new();
        for version in version_directories(&plugins_root(data_dir))? {
            let Some(id) = version
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
            else {
                continue;
            };
            let fingerprint =
                read_bounded_file(&version.join(MANIFEST_MEMBER), MAX_MANIFEST_BYTES as u64)
                    .and_then(|bytes| parse_manifest(&bytes))
                    .and_then(|manifest| decode_public_key(&manifest.public_key))
                    .map_or_else(|_| UNREADABLE.to_owned(), |key| key_fingerprint(&key));
            entries.push((id.to_owned(), fingerprint));
        }
        Ok(Self::from_entries(entries))
    }

    /// From `(plugin id, key fingerprint)` pairs.
    #[must_use]
    pub fn from_entries(entries: impl IntoIterator<Item = (String, String)>) -> Self {
        let mut keys: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (id, fingerprint) in entries {
            keys.entry(id).or_default().insert(fingerprint);
        }
        Self(Arc::new(keys))
    }

    /// Whether `id` is installed under a key other than `fingerprint`; `None` is a key that does
    /// not read, which matches none.
    #[must_use]
    pub fn changed(&self, id: &str, fingerprint: Option<&str>) -> bool {
        self.0.get(id).is_some_and(|installed| {
            installed
                .iter()
                .any(|known| known == UNREADABLE || fingerprint != Some(known.as_str()))
        })
    }
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
