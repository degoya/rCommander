//! The bundled packages a start judged already (RC-110-64): `<data dir>/plugins/bundled-seen.json`.
//!
//! Reading and verifying every bundled package - SHA-256, signature, component - and then the
//! installed copy beside it cost every start the same, though both change only with an update.
//! A package whose file still has the size and modification time recorded here, judged under the
//! same roots and the same trust (the person's keys and withdrawals, development mode), whose
//! installed version folder is still there, is passed over. Anything else is checked as before
//! and recorded anew. A record that does not read, or was made under other trust, is no record.
//!
//! Only an outcome that needs nothing more is recorded: a package installed, replaced or left
//! alone - never one that did not verify or whose install or replacement failed, so the next
//! start tries it again.

use std::{collections::BTreeMap, io::Write as _, path::Path, time::UNIX_EPOCH};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

use super::super::{PluginState, disk::read_optional_bounded, plugins_root};
use crate::{PluginInstaller, VerifiedPackage, format_package_digest, sha256_hex, sign::root_keys};

/// The record, beside `state.json`.
pub(super) const SEEN_FILE: &str = "bundled-seen.json";
/// The largest record read; one line per bundled package is far below it.
const MAX_SEEN_BYTES: u64 = 1024 * 1024;

/// A file's size and modification time.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct FileStamp {
    size: u64,
    secs: u64,
    nanos: u32,
}

impl FileStamp {
    /// The stamp of `file`; `None` when its metadata or time does not read.
    pub(super) fn of(file: &Path) -> Option<Self> {
        let metadata = std::fs::metadata(file).ok()?;
        let since = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
        Some(Self {
            size: metadata.len(),
            secs: since.as_secs(),
            nanos: since.subsec_nanos(),
        })
    }
}

/// One bundled package as a start judged it: its file's stamp, what it installs and its digest.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Judged {
    stamp: FileStamp,
    id: String,
    version: String,
    digest: String,
}

/// The judged packages by file, under the trust they were judged with.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct Seen {
    context: String,
    packages: BTreeMap<String, Judged>,
}

impl Seen {
    /// An empty record for this start's `context`.
    pub(super) fn new(context: String) -> Self {
        Self {
            context,
            packages: BTreeMap::new(),
        }
    }

    /// The record of the last start when it reads and was made under `context`; else empty.
    pub(super) fn read(data_dir: &Path, context: &str) -> Self {
        let path = plugins_root(data_dir).join(SEEN_FILE);
        let read = read_optional_bounded(&path, MAX_SEEN_BYTES).and_then(|bytes| {
            bytes
                .map(|bytes| serde_json::from_slice::<Self>(&bytes).map_err(anyhow::Error::from))
                .transpose()
        });
        match read {
            Ok(Some(seen)) if seen.context == context => seen,
            Ok(_) => Self::new(context.to_owned()),
            Err(error) => {
                tracing::debug!(%error, "the record of the bundled plugins does not read; all are checked");
                Self::new(context.to_owned())
            }
        }
    }

    /// The record of `file` when the file is unchanged since and its version is still installed.
    pub(super) fn unchanged(
        &self,
        file: &Path,
        stamp: Option<FileStamp>,
        installer: &PluginInstaller,
    ) -> Option<&Judged> {
        let judged = self.packages.get(&key(file))?;
        let unchanged = stamp == Some(judged.stamp)
            && installer
                .version_dir(&judged.id, &judged.version)
                .is_ok_and(|dir| dir.is_dir());
        unchanged.then_some(judged)
    }

    /// Records `package`, read from `file` with `stamp`; a file without a stamp is not recorded.
    pub(super) fn record(
        &mut self,
        file: &Path,
        stamp: Option<FileStamp>,
        package: &VerifiedPackage,
    ) {
        let Some(stamp) = stamp else {
            return;
        };
        self.packages.insert(
            key(file),
            Judged {
                stamp,
                id: package.manifest.id.clone(),
                version: package.manifest.version.clone(),
                digest: format_package_digest(&package.digest()),
            },
        );
    }

    /// Keeps the last start's record of `file`, passed over this time.
    pub(super) fn keep(&mut self, file: &Path, judged: Judged) {
        self.packages.insert(key(file), judged);
    }

    /// Whether `file` is recorded.
    #[cfg(test)]
    pub(super) fn holds(&self, file: &Path) -> bool {
        self.packages.contains_key(&key(file))
    }

    /// Writes the record in place of the last one: a temporary file renamed over it.
    ///
    /// # Errors
    ///
    /// The plugins folder cannot be made or the file cannot be written.
    pub(super) fn write(&self, data_dir: &Path) -> Result<()> {
        let root = plugins_root(data_dir);
        std::fs::create_dir_all(&root).with_context(|| format!("create {}", root.display()))?;
        let bytes = serde_json::to_vec_pretty(self)?;
        let temporary = root.join(format!(".{SEEN_FILE}.tmp"));
        let mut file = std::fs::File::create(&temporary)
            .with_context(|| format!("write {}", temporary.display()))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, root.join(SEEN_FILE))
            .with_context(|| format!("replace {}", root.join(SEEN_FILE).display()))
    }
}

/// What a judgement rests on besides the files: the roots, and the trust `state` adds to them.
/// A change of either judges every package again.
pub(super) fn context(roots: &str, state: &PluginState) -> String {
    let trust = serde_json::to_vec(&(
        roots,
        state.development_mode,
        &state.trusted_keys,
        &state.revoked_keys,
        &state.revoked_digests,
    ))
    .expect("strings, a flag and maps and sets of strings always encode");
    sha256_hex(&trust)
}

/// The compiled-in roots this build trusts now, as text for [`context`].
pub(super) fn roots_stamp() -> String {
    root_keys()
        .iter()
        .map(|root| format!("{}={}", root.key_id, root.public_key))
        .collect::<Vec<_>>()
        .join(",")
}

fn key(file: &Path) -> String {
    file.to_string_lossy().into_owned()
}

#[cfg(test)]
#[path = "seen_tests.rs"]
mod tests;
