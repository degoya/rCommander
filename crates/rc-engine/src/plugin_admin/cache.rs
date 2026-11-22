//! The plugins' compiled code kept between starts (RC-110-48): `<data dir>/plugin-cache`.
//!
//! Configured when the engine is given its plugin folder ([`crate::Engine::attach_plugins`]):
//! every way the engine compiles a plugin - the start's bundled packages, the viewers, the
//! secret sources, the inventories, the compare plugins, an install's preview - needs that
//! folder first, so nothing can compile before the cache is there. Thinned out once per start
//! after the sweep, on the blocking pool and not waited for.

use std::path::{Path, PathBuf};

use rc_plugin_host::{CachePrune, installed};

use crate::recover::LogPanic as _;

/// The folder under the data folder that holds the compiled plugin code.
pub(crate) const PLUGIN_CACHE_DIR: &str = "plugin-cache";

/// Configures the process's compile cache under `data_dir`. A failure leaves every plugin
/// compiling at every start: slower, not wrong.
pub(super) fn configure(data_dir: &Path) {
    let directory = data_dir.join(PLUGIN_CACHE_DIR);
    if let Err(error) = rc_plugin_host::configure_compile_cache(&directory) {
        tracing::warn!(
            error = %format!("{error:#}"),
            "the plugin compile cache is off; plugins compile at every start"
        );
    }
}

/// One pass over the compile cache by the components installed under `data_dir` now: the
/// entries of another Wasmtime and of components no longer installed go. Blocking.
pub(crate) fn prune(data_dir: &Path) -> CachePrune {
    match installed::component_digests(data_dir) {
        Ok(digests) => {
            let pruned = rc_plugin_host::prune_compile_cache(&digests, false);
            if pruned.removed_entries > 0 {
                tracing::info!(
                    removed = pruned.removed_entries,
                    bytes = pruned.removed_bytes,
                    "the plugin compile cache was pruned"
                );
            }
            pruned
        }
        Err(error) => {
            // Without the installed components nothing is known to be unused.
            tracing::warn!(
                error = %format!("{error:#}"),
                "the plugin compile cache was not pruned"
            );
            CachePrune::default()
        }
    }
}

/// [`prune`] on the blocking pool; the start does not wait for it.
pub(super) fn prune_in_background(data_dir: PathBuf) {
    tokio::spawn(async move {
        let _ = tokio::task::spawn_blocking(move || prune(&data_dir))
            .await
            .log_panic("engine.plugin_cache_prune");
    });
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
