//! Packages for tests: a working viewer component, one importing what no viewer is granted, and
//! signed or unsigned `.rcplug` packages of them - for this crate's tests and, through the
//! `test-support` feature, those of `rc-engine` and `rcommander`. Never in a shipped build.

use ed25519_dalek::SigningKey;

use crate::{package_plugin, public_key_base64, testbed};

/// A viewer that works: `open` offers `text`, `text` writes `hello`.
#[must_use]
pub fn viewer_component() -> Vec<u8> {
    testbed::Testbed::new().build()
}

/// The same viewer built against the current contract, for a manifest naming it - the SDK's
/// scaffold and the bundled plugins say `api_version = "0.3.0"`.
#[must_use]
pub fn current_viewer_component() -> Vec<u8> {
    testbed::Testbed::new()
        .contract(rc_plugin_api::API_VERSION)
        .build()
}

/// A secret source built against the current contract that answers without a service: ready,
/// no entries, a fixed value (RC-100-33) - standing in for the SDK's `secrets` scaffold.
#[must_use]
pub fn current_secrets_component() -> Vec<u8> {
    testbed::secrets::SecretsBed::new().build()
}

/// A component importing `http`, which no viewer is granted.
#[must_use]
pub fn component_importing_http() -> Vec<u8> {
    testbed::importing_http()
}

/// A signing key from a fixed seed: the same key in every run.
#[must_use]
pub fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

/// A valid viewer manifest of plugin `id` (a lower-case UUIDv7) at `version`, signed by `key`
/// under `key_id`, claiming `.csv` and `.tsv`.
#[must_use]
pub fn viewer_manifest(id: &str, version: &str, key_id: &str, key: &SigningKey) -> String {
    let public_key = public_key_base64(key);
    format!(
        r#"manifest_version = 1
plugin_type = "viewer"
api_version = "0.1.0"
id = "{id}"
name = "CSV table"
version = "{version}"
key_id = "{key_id}"
public_key = "{public_key}"

[match]
extensions = ["csv", "tsv"]

[viewer]
outputs = ["text"]

[metadata]
description = "Shows CSV files as a table."
author = "rCommander tests"
license = "MIT"
min_app_version = "0.1.0"

[[settings]]
key = "separator"
type = "choice"
values = ["auto", "comma", "semicolon"]
default = "auto"
"#
    )
}

/// A package of [`viewer_component`] with [`viewer_manifest`], signed by `key` - or unsigned with
/// `signed` off, which only development mode installs.
///
/// # Panics
///
/// The packager refuses it, which a test helper may.
#[must_use]
pub fn viewer_package(
    id: &str,
    version: &str,
    key_id: &str,
    key: &SigningKey,
    signed: bool,
) -> Vec<u8> {
    let manifest = viewer_manifest(id, version, key_id, key);
    package_plugin(
        manifest.as_bytes(),
        &viewer_component(),
        &[],
        signed.then_some(key),
    )
    .expect("a valid test package")
}

/// A package of the secret source `component` (a `testbed::secrets::SecretsBed` built, for
/// example) as plugin `id` named `Test source`, signed by `key` under `key_id`, with a 1 s time
/// limit per call and no `http` (RC-100-34's tests).
///
/// # Panics
///
/// The packager refuses it, which a test helper may.
#[must_use]
pub fn secrets_package(id: &str, key_id: &str, key: &SigningKey, component: &[u8]) -> Vec<u8> {
    secrets_package_with(id, key_id, key, component, "")
}

/// [`secrets_package`] with `extra` tables after the manifest's own (`[[settings]]`, RC-100-36).
///
/// # Panics
///
/// The packager refuses it, which a test helper may.
#[must_use]
pub fn secrets_package_with(
    id: &str,
    key_id: &str,
    key: &SigningKey,
    component: &[u8],
    extra: &str,
) -> Vec<u8> {
    let public_key = public_key_base64(key);
    let manifest = format!(
        r#"manifest_version = 1
plugin_type = "secrets"
api_version = "{api}"
id = "{id}"
name = "Test source"
version = "1.0.0"
key_id = "{key_id}"
public_key = "{public_key}"

[metadata]
description = "A secret source for tests."
author = "rCommander tests"
license = "MIT"
min_app_version = "0.1.0"

[limits]
timeout_milliseconds = 1000
{extra}"#,
        api = rc_plugin_api::API_VERSION,
    );
    package_plugin(manifest.as_bytes(), component, &[], Some(key)).expect("a valid test package")
}
