//! RC-110-48: the plugin folder brings its compile cache, and the start's pass prunes it. The
//! compile cache is the process's: nextest runs each test in a process of its own.

use std::{path::Path, sync::Arc};

use rc_plugin_host::{ExecutionLog, testbed::Testbed};

use super::{PLUGIN_CACHE_DIR, prune};
use crate::{
    plugins::test_support::{Spec, plugin},
    test_support,
};

/// The compiled entries under the cache folder: `wasmtime/modules/<release>/<entry>`, an
/// entry's name 43 characters long (Wasmtime's usage files beside them end in `.stats`).
fn entries(cache: &Path) -> usize {
    let modules = cache.join("wasmtime").join("modules");
    let Ok(releases) = std::fs::read_dir(modules) else {
        return 0;
    };
    releases
        .flatten()
        .filter_map(|release| std::fs::read_dir(release.path()).ok())
        .flat_map(|files| files.flatten())
        .filter(|file| file.file_name().len() == 43)
        .count()
}

#[tokio::test]
async fn the_plugin_folder_brings_its_compile_cache_and_the_start_prunes_it() {
    let root = tempfile::tempdir().expect("temp dir");
    let engine = test_support::headless(root.path()).await;
    let data = root.path().join("data");
    assert!(engine.attach_plugins(data.clone(), None));
    let cache = data.join(PLUGIN_CACHE_DIR);
    assert!(cache.is_dir(), "configured with the plugin folder");

    let log = Arc::new(ExecutionLog::new());
    let _viewer = plugin(&Spec::default(), &Testbed::new(), &log);
    let written = entries(&cache);
    assert!(written >= 1, "the compile is kept for the next start");

    // Nothing is installed under `data`: no entry belongs to an installed component.
    let pruned = prune(&data);
    assert_eq!(pruned.removed_entries, written as u64);
    assert_eq!(entries(&cache), 0);
}
