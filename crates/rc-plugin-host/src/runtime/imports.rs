//! Which WIT interfaces a component may import: the base every plugin gets, the interfaces its
//! kind is made of, and one per granted capability - each under the contract version its
//! manifest names. Everything else - WASI and another version included - is refused when the
//! component is compiled.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/runtime/imports.rs` at commit
//! 26c8592a6: the same rule (one grant, one interface), rCommander's package and kinds. Stage 1
//! has no capability a viewer may hold, so a viewer imports `host`, `types`, `source` and `sink`,
//! and `settings` when its manifest declares `[[settings]]` (RC-100-14), and nothing else. A
//! secret source imports `host`, `types`, `settings` as a viewer does, and `http` when granted
//! `net_http` (RC-100-33).

use crate::manifest::{NET_HTTP, PluginManifest, PluginType};

/// Interfaces every plugin may import: the log, a clock, random bytes, the shared types.
const BASE_IMPORTS: [&str; 2] = ["rcommander:plugin/host", "rcommander:plugin/types"];

/// The contract before `settings` (RC-100-14): a plugin built against it imports none.
const BEFORE_SETTINGS: &str = "0.1.0";

/// The interface a plugin reads its own `[[settings]]` through.
const SETTINGS: &str = "rcommander:plugin/settings";

/// The interface `net_http` grants (contract 0.3.0, RC-100-33).
const HTTP: &str = "rcommander:plugin/http";

/// Whether `manifest` may import `http`: a secret source granted `net_http`.
pub(crate) fn http_granted(manifest: &PluginManifest) -> bool {
    manifest.plugin_type == PluginType::Secrets
        && manifest.capabilities.declared.contains_key(NET_HTTP)
}

/// Whether `manifest` may import `settings`: it declares `[[settings]]` under a contract that
/// has the interface. Like rDownloader's `destination-settings`, the values were checked against
/// the manifest and reach nothing, but a plugin without settings has no reason to ask.
pub(crate) fn settings_granted(manifest: &PluginManifest) -> bool {
    !manifest.settings.is_empty() && manifest.api_version != BEFORE_SETTINGS
}

/// The interfaces one manifest permits, each with its version: `rcommander:plugin/host@0.3.0`.
pub(crate) fn allowed_imports(manifest: &PluginManifest) -> Vec<String> {
    let mut allowed = BASE_IMPORTS.to_vec();
    // The source a viewer reads and the sink it writes are what a viewer is, not a grant: both
    // are handles the host opened for the call at hand, so neither names a file.
    if manifest.plugin_type == PluginType::Viewer {
        allowed.extend(["rcommander:plugin/source", "rcommander:plugin/sink"]);
    }
    if settings_granted(manifest) {
        allowed.push(SETTINGS);
    }
    if http_granted(manifest) {
        allowed.push(HTTP);
    }
    allowed
        .into_iter()
        .map(|interface| versioned(interface, &manifest.api_version))
        .collect()
}

/// `interface` of contract `api_version`, as a component names its import.
pub(crate) fn versioned(interface: &str, api_version: &str) -> String {
    format!("{interface}@{api_version}")
}
