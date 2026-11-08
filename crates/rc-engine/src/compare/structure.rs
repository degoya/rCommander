//! Compare by structure (RC-100-41): two files a compare plugin claims - JSON, CSV - compared as
//! a person reads the data, beside the line diff, which stays the core's.
//!
//! The first installed, enabled compare plugin whose `[match]` claims both files compares them.
//! Both are read whole, up to `plugins.compare_limit_mib`, and handed to it as its two sources;
//! a key or credential file - what the MCP read guard refuses, or a file holding a private key -
//! never reaches a plugin (`docs/plugins.md`, decision 8). The plugin is asked `status` first: a
//! paid plugin without a valid licence key says `unlicensed`, and the window shows the line diff
//! alone. Every outcome is an answer, never an error: a structural comparison that cannot run
//! leaves the line comparison as it was.

use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use rc_plugin_host::{
    BytesSource, CompareOptions, ComparePlugin, ExecutionLog, InstalledPlugin, PluginLocale,
    PluginType, SourceReader,
};
use rc_vfs::{VfsError, VfsPath};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt as _;
use tokio_util::sync::CancellationToken;

pub use rc_plugin_api::compare::{Change, ChangeKind, CompareSummary, Comparison, Notice};

use super::{Side, file_entry};
use crate::recover::LogPanic as _;
use crate::{
    Engine, EngineError,
    plugins::{failure_code, stored_plugin_settings},
    read_guard::ReadGuard,
    settings::{COMPARE_LIMIT_KEY, DEFAULT_COMPARE_LIMIT_MIB, LANGUAGE_KEY, stored_setting},
    viewer::secret,
};

const MIB: u64 = 1024 * 1024;

/// Bytes of a file's head searched for a private key.
const HEAD_BYTES: usize = 4096;

/// What a structural comparison of two files came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum StructureOutcome {
    /// The plugin compared them.
    Compared {
        /// The plugin's name, as its manifest gives it.
        plugin: String,
        plugin_id: String,
        comparison: Comparison,
        /// The comparison's notices in the plugin's words (its `locales/<lang>.json`, the
        /// interface's language, else English), each parameter filled in.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        notices: Vec<String>,
    },
    /// No installed, enabled compare plugin claims both files (or the plugins are off).
    NoPlugin,
    /// The plugin that claims them has no valid licence key.
    Unlicensed { plugin: String },
    /// Either file is larger than `plugins.compare_limit_mib`.
    TooLarge { limit_mib: u64 },
    /// Either file is a key or credential file, which no plugin is handed.
    Sensitive,
    /// The plugin failed: `code` is its failure's (`plugin_crashed`, `plugin_compare_too_large`,
    /// `plugin_failed` for its own words, ...).
    Failed { plugin: String, code: String },
}

impl StructureOutcome {
    /// The comparison, when the plugin made one.
    #[must_use]
    pub fn comparison(&self) -> Option<&Comparison> {
        match self {
            Self::Compared { comparison, .. } => Some(comparison),
            _ => None,
        }
    }
}

impl Engine {
    /// `left` and `right` compared by the first compare plugin that claims both; see the module.
    ///
    /// # Errors
    ///
    /// Either file cannot be read or is a directory, or `cancel` was cancelled.
    pub async fn structure_paths(
        &self,
        left: &VfsPath,
        right: &VfsPath,
        cancel: &CancellationToken,
    ) -> Result<StructureOutcome, EngineError> {
        let (left_vfs, right_vfs) = (self.vfs(left)?, self.vfs(right)?);
        let sides = [
            Side {
                vfs: &*left_vfs,
                path: left,
            },
            Side {
                vfs: &*right_vfs,
                path: right,
            },
        ];
        let left_entry = file_entry(sides[0]).await?;
        let right_entry = file_entry(sides[1]).await?;
        let named = [
            (file_name(left), left_entry.size),
            (file_name(right), right_entry.size),
        ];
        let claimed = self
            .installed_of(PluginType::Compare)
            .await
            .into_iter()
            .find(|package| {
                named
                    .iter()
                    .all(|(name, size)| package.manifest.matching.matches(name, &[], *size))
            });
        let Some(package) = claimed else {
            return Ok(StructureOutcome::NoPlugin);
        };
        let limit_mib = self.compare_limit_mib().await;
        let limit = limit_mib.saturating_mul(MIB);
        if named.iter().any(|(_, size)| *size > limit) {
            return Ok(StructureOutcome::TooLarge { limit_mib });
        }
        let mut sources: Vec<Arc<dyn SourceReader>> = Vec::with_capacity(2);
        for (side, (name, _)) in sides.iter().zip(&named) {
            if self.refused(side.path).await {
                return Ok(StructureOutcome::Sensitive);
            }
            let Some(bytes) = read_within(*side, limit, cancel).await? else {
                return Ok(StructureOutcome::TooLarge { limit_mib });
            };
            if secret::has_private_key(&bytes[..bytes.len().min(HEAD_BYTES)]) {
                return Ok(StructureOutcome::Sensitive);
            }
            sources.push(Arc::new(BytesSource::new(name, bytes)));
        }
        let (Some(b), Some(a)) = (sources.pop(), sources.pop()) else {
            return Ok(StructureOutcome::NoPlugin);
        };
        Ok(self.run_compare(package, a, b, cancel).await)
    }

    /// `package` built and asked: `status`, then `compare` of `a` and `b`.
    async fn run_compare(
        &self,
        package: InstalledPlugin,
        a: Arc<dyn SourceReader>,
        b: Arc<dyn SourceReader>,
        cancel: &CancellationToken,
    ) -> StructureOutcome {
        let plugin = package.manifest.name.clone();
        let plugin_id = package.manifest.id.clone();
        let locales = package.version_dir.join("locales");
        let failed = |code: &str| StructureOutcome::Failed {
            plugin: plugin.clone(),
            code: code.to_owned(),
        };
        let options = CompareOptions {
            log: self
                .plugin_execution_log()
                .unwrap_or_else(|| Arc::new(ExecutionLog::new())),
            health: self.plugin_health(&package.manifest.id).await,
            settings: stored_plugin_settings(
                self.db().ok(),
                &package.manifest.id,
                &package.manifest.settings,
            )
            .await,
        };
        let built = tokio::task::spawn_blocking(move || {
            ComparePlugin::new(&package.manifest, &package.component, options)
        })
        .await
        .log_panic("engine.compare_plugin_build");
        let compare = match built {
            Ok(Ok(compare)) => compare,
            Ok(Err(failure)) => return failed(failure_code(failure.code())),
            Err(_) => return failed(crate::plugins::PLUGIN_FAILED),
        };
        match Box::pin(compare.status()).await {
            Ok(rc_plugin_api::compare::CompareState::Ready) => {}
            Ok(rc_plugin_api::compare::CompareState::Unlicensed) => {
                return StructureOutcome::Unlicensed {
                    plugin: plugin.clone(),
                };
            }
            Err(failure) => return failed(failure_code(failure.code())),
        }
        // The window's token raised asks the plugin to stop, as Quick View moving on does.
        let stop = Arc::new(AtomicBool::new(false));
        let watcher = {
            let (stop, cancel) = (Arc::clone(&stop), cancel.clone());
            tokio::spawn(async move {
                cancel.cancelled().await;
                stop.store(true, Ordering::Relaxed);
            })
        };
        let compared = Box::pin(compare.compare(a, b, stop)).await;
        watcher.abort();
        match compared {
            Ok(comparison) => {
                let notices = if comparison.notices.is_empty() {
                    Vec::new()
                } else {
                    let codes = plugin_codes(&locales, &self.language().await).await;
                    comparison
                        .notices
                        .iter()
                        .map(|notice| notice_text(&codes, notice))
                        .collect()
                };
                StructureOutcome::Compared {
                    plugin: plugin.clone(),
                    plugin_id,
                    comparison,
                    notices,
                }
            }
            Err(failure) => failed(failure_code(failure.code())),
        }
    }

    /// `plugins.compare_limit_mib`: stored, or its default.
    async fn compare_limit_mib(&self) -> u64 {
        match self.db() {
            Ok(db) => stored_setting(db, COMPARE_LIMIT_KEY)
                .await
                .and_then(|value| value.as_u64())
                .filter(|mib| *mib > 0)
                .unwrap_or(DEFAULT_COMPARE_LIMIT_MIB),
            Err(_) => DEFAULT_COMPARE_LIMIT_MIB,
        }
    }

    /// The interface's language when one is chosen (`appearance.language`), else English.
    async fn language(&self) -> String {
        let chosen = match self.db() {
            Ok(db) => stored_setting(db, LANGUAGE_KEY)
                .await
                .and_then(|value| value.as_str().map(str::to_owned)),
            Err(_) => None,
        };
        chosen
            .filter(|code| code.len() == 2 && code.bytes().all(|byte| byte.is_ascii_lowercase()))
            .unwrap_or_else(|| "en".to_owned())
    }

    /// Whether the judge MCP runs under, when one is attached, refuses `path`.
    async fn refused(&self, path: &VfsPath) -> bool {
        match self.inner.read_judge.get() {
            Some(judge) => ReadGuard::new(*judge, &path.backend)
                .refuses_off_thread(&path.path)
                .await
                .is_some(),
            None => false,
        }
    }
}

/// The plugin's texts by code: `locales/<language>.json` of its installed version, else
/// `en.json`; none when it ships neither. Both were checked when the package was installed.
async fn plugin_codes(locales: &Path, language: &str) -> BTreeMap<String, String> {
    for name in [language, "en"] {
        let Ok(bytes) = tokio::fs::read(locales.join(format!("{name}.json"))).await else {
            continue;
        };
        if let Ok(locale) = serde_json::from_slice::<PluginLocale>(&bytes) {
            return locale.codes;
        }
    }
    BTreeMap::new()
}

/// A notice in the plugin's words, each `{param}` filled in; the code itself without a text.
fn notice_text(codes: &BTreeMap<String, String>, notice: &Notice) -> String {
    let Some(text) = codes.get(&notice.code) else {
        return notice.code.clone();
    };
    notice
        .params
        .iter()
        .fold(text.clone(), |text, (key, value)| {
            text.replace(&format!("{{{key}}}"), value)
        })
}

/// The file's name, never its folder: what the plugin's `[match]` and `source.info` see.
fn file_name(path: &VfsPath) -> String {
    path.path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A whole file, or `None` when it is larger than `limit` bytes.
async fn read_within(
    side: Side<'_>,
    limit: u64,
    cancel: &CancellationToken,
) -> Result<Option<Vec<u8>>, EngineError> {
    if cancel.is_cancelled() {
        return Err(EngineError::Cancelled);
    }
    let stream = side.vfs.open_read(&side.path.path).await?;
    let mut bytes = Vec::new();
    stream
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| VfsError::from_io(error, &side.path.path))?;
    Ok((bytes.len() as u64 <= limit).then_some(bytes))
}

#[cfg(test)]
#[path = "structure_tests.rs"]
mod tests;
