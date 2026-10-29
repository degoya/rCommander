//! The installed viewer plugins and which of them shows a file.
//!
//! Adapted from rDownloader's `crates/rd-plugin-ext/src/plugin_set.rs` at commit 26c8592a6:
//! `PluginSet<P>` (one entry per installed plugin, the first version of an id wins, a plugin
//! that fails to build is logged and left out, costing its own feature and nothing else) and
//! `ClaimedPlugins<P>` (what each plugin claims, looked up without running it). Here a claim is
//! the manifest's `[match]` - extension, magic, `max_size` - with `plugins.associations` before
//! every manifest and the built-in formats first (`docs/plugins.md`, decision 11), and a file has
//! a list of claims, asked in turn: an extension before a bare magic (decision 16, RC-100-27).

use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicUsize},
};

use rc_plugin_host::{ExecutionLog, PluginHealth, PluginManifest, ViewerPlugin};

use crate::viewer::sniff;

/// The most leading bytes read to match a file: a magic further in claims nothing.
pub const MAX_HEAD_BYTES: usize = 64 * 1024;

/// Leading bytes the built-in image check needs.
const SNIFF_BYTES: usize = 64;

/// The most plugins asked in turn about one file: a declining `probe` hands the file to the
/// next, and Quick View waits for each.
pub const MAX_CANDIDATES: usize = 4;

/// Extensions the built-in Markdown preview shows (`markdown.rs` in the window).
const MARKDOWN_EXTENSIONS: [&str; 2] = ["md", "markdown"];

/// One installed viewer plugin, compiled, with the manifest it was built from.
pub struct InstalledViewer {
    pub(crate) manifest: PluginManifest,
    pub(crate) plugin: ViewerPlugin,
    /// The version folder an open document holds (`rc_plugin_host::installed::hold`), so the
    /// version is not removed under it; `None` for a plugin built elsewhere.
    pub(crate) version_dir: Option<PathBuf>,
}

impl InstalledViewer {
    /// `plugin`, built from `manifest` outside the installed folders.
    #[must_use]
    pub fn new(manifest: PluginManifest, plugin: ViewerPlugin) -> Self {
        Self {
            manifest,
            plugin,
            version_dir: None,
        }
    }

    /// The same, installed in `version_dir`.
    #[must_use]
    pub fn installed(manifest: PluginManifest, plugin: ViewerPlugin, version_dir: &Path) -> Self {
        Self {
            version_dir: Some(version_dir.to_path_buf()),
            ..Self::new(manifest, plugin)
        }
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    /// The manifest's name, for the notice when it fails.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    /// Its failure count and suspension; the person's Resume is RC-100-09's.
    #[must_use]
    pub fn health(&self) -> &Arc<PluginHealth> {
        self.plugin.health()
    }
}

/// The installed, enabled viewer plugins, sorted by name.
pub struct ViewerPlugins {
    pub(crate) plugins: Vec<Arc<InstalledViewer>>,
    associations: BTreeMap<String, String>,
    log: Arc<ExecutionLog>,
    /// Documents open now, for the tests and the diagnostics.
    pub(crate) live: Arc<AtomicUsize>,
}

impl std::fmt::Debug for ViewerPlugins {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ids: Vec<&str> = self.plugins.iter().map(|viewer| viewer.id()).collect();
        f.debug_struct("ViewerPlugins")
            .field("plugins", &ids)
            .field("associations", &self.associations)
            .finish_non_exhaustive()
    }
}

impl ViewerPlugins {
    /// No plugin: every file in the built-in viewer.
    #[must_use]
    pub fn none() -> Self {
        Self::from_plugins(Vec::new(), BTreeMap::new(), Arc::new(ExecutionLog::new()))
    }

    /// The set of `plugins` built elsewhere: the first of an id wins, sorted by name so the
    /// plugin that claims a file does not depend on the order on disk.
    #[must_use]
    pub fn from_plugins(
        plugins: Vec<InstalledViewer>,
        associations: BTreeMap<String, String>,
        log: Arc<ExecutionLog>,
    ) -> Self {
        let mut seen = HashSet::new();
        let mut plugins: Vec<Arc<InstalledViewer>> = plugins
            .into_iter()
            .filter(|viewer| seen.insert(viewer.manifest.id.clone()))
            .map(Arc::new)
            .collect();
        plugins.sort_by(|left, right| (left.name(), left.id()).cmp(&(right.name(), right.id())));
        Self {
            plugins,
            associations,
            log,
            live: Arc::new(AtomicUsize::new(0)),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// The plugins, sorted by name.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<InstalledViewer>> {
        self.plugins.iter()
    }

    /// Every call of every plugin of the set: codes and outcomes, never a message.
    #[must_use]
    pub fn execution_log(&self) -> &Arc<ExecutionLog> {
        &self.log
    }

    /// Plugin documents open now.
    #[cfg(test)]
    #[must_use]
    pub fn live_sessions(&self) -> usize {
        self.live.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// How many leading bytes [`ViewerPlugins::claims`] needs to see.
    #[must_use]
    pub fn head_length(&self) -> usize {
        self.plugins
            .iter()
            .map(|viewer| viewer.manifest.matching.head_length())
            .fold(SNIFF_BYTES, usize::max)
            .min(MAX_HEAD_BYTES)
    }

    /// The plugins asked in turn to show the file `name` of `size` bytes starting with `head`,
    /// decided without running one; empty for the built-in viewer. First the plugin
    /// `plugins.associations` assigns the extension to (within its `max_size`); then, unless the
    /// file is an image or Markdown, which stay built in, the plugins whose extension matches,
    /// then those whose magic matches, each by name. No plugin twice, at most
    /// [`MAX_CANDIDATES`].
    #[must_use]
    pub fn claims(&self, name: &str, head: &[u8], size: u64) -> Vec<Arc<InstalledViewer>> {
        let extension = name
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase());
        let mut claims: Vec<Arc<InstalledViewer>> = extension
            .as_deref()
            .and_then(|extension| self.associations.get(extension))
            .and_then(|id| self.plugins.iter().find(|viewer| viewer.id() == id))
            .filter(|viewer| {
                viewer
                    .manifest
                    .matching
                    .max_size
                    .is_none_or(|max| size <= max)
            })
            .cloned()
            .into_iter()
            .collect();
        let markdown = extension
            .as_deref()
            .is_some_and(|extension| MARKDOWN_EXTENSIONS.contains(&extension));
        if markdown || sniff::sniff_image(head).is_some() {
            return claims;
        }
        // `matches` with an empty head asks the extension alone (a magic is never empty), with a
        // name without a dot the magic alone; both keep `max_size`.
        let by_extension = self
            .plugins
            .iter()
            .filter(|viewer| viewer.manifest.matching.matches(name, &[], size));
        let by_magic = self
            .plugins
            .iter()
            .filter(|viewer| viewer.manifest.matching.matches("", head, size));
        for viewer in by_extension.chain(by_magic) {
            if claims.len() == MAX_CANDIDATES {
                break;
            }
            if !claims.iter().any(|claimed| Arc::ptr_eq(claimed, viewer)) {
                claims.push(Arc::clone(viewer));
            }
        }
        claims
    }
}

#[cfg(test)]
#[path = "set_tests.rs"]
mod tests;
