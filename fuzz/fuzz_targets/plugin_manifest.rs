//! A plugin's `manifest.toml` (`rc_plugin_host::parse_manifest`), and the app version check of a
//! manifest that passed.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rc_plugin_host::{APP_VERSION, check_app_version, parse_manifest};

fuzz_target!(|data: &[u8]| {
    if let Ok(manifest) = parse_manifest(data) {
        let verdict = check_app_version(&manifest, APP_VERSION);
        std::hint::black_box(verdict.is_ok());
    }
});
