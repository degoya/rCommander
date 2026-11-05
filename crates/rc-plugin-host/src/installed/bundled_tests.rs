use std::path::{Path, PathBuf};

use anyhow::Result;

use super::{BUNDLED_MARKER, bundled_dir_of, sync_with};
use crate::{
    PluginInstaller, PluginState, PluginVerifier,
    installed::{entry, plugins_root},
    key_fingerprint,
    package::MANIFEST_MEMBER,
    public_key_base64,
    test_fixtures::PLUGIN_ID,
    test_support::{signing_key, viewer_package},
};

/// The key an older build shipped its plugins under, and the one this build trusts.
const OLD_KEY_ID: &str = "example-dev-v1";
const OLD: u8 = 5;
const NEW_KEY_ID: &str = "example-plugin-v1";
const NEW: u8 = 7;

#[test]
fn the_packages_lie_beside_a_plain_executable() {
    assert_eq!(
        bundled_dir_of(Path::new("/opt/rcommander/rcommander")),
        Some(PathBuf::from("/opt/rcommander/plugins"))
    );
}

#[test]
fn an_app_bundle_keeps_them_in_its_resources() {
    assert_eq!(
        bundled_dir_of(Path::new(
            "/Applications/rCommander.app/Contents/MacOS/rcommander"
        )),
        Some(PathBuf::from(
            "/Applications/rCommander.app/Contents/Resources/plugins"
        ))
    );
}

#[test]
fn a_folder_named_macos_outside_a_bundle_is_a_plain_folder() {
    assert_eq!(
        bundled_dir_of(Path::new("/home/me/MacOS/rcommander")),
        Some(PathBuf::from("/home/me/MacOS/plugins"))
    );
}

/// A verifier standing in for the roots: the key of `seed` under `key_id`, nothing else.
fn roots(key_id: &'static str, seed: u8) -> impl Fn() -> Result<PluginVerifier> {
    move || {
        let verifier = PluginVerifier::new(false);
        verifier.trust_key_base64(key_id.to_owned(), &public_key_base64(&signing_key(seed)))?;
        Ok(verifier)
    }
}

/// A folder of bundled packages holding version 1.0.0, signed by `seed` under `key_id`.
fn shipped(key_id: &str, seed: u8) -> tempfile::TempDir {
    let folder = tempfile::tempdir().expect("bundled");
    let package = viewer_package(PLUGIN_ID, "1.0.0", key_id, &signing_key(seed), true);
    std::fs::write(folder.path().join("csv.rcplug"), package).expect("write");
    folder
}

fn version_dir(data: &Path) -> PathBuf {
    plugins_root(data).join(PLUGIN_ID).join("1.0.0")
}

/// The public key the installed version's manifest names.
fn installed_key(data: &Path) -> String {
    let bytes = std::fs::read(version_dir(data).join(MANIFEST_MEMBER)).expect("manifest");
    crate::parse_manifest(&bytes).expect("parse").public_key
}

fn key_of(seed: u8) -> String {
    public_key_base64(&signing_key(seed))
}

/// An older build's start: version 1.0.0 installed as bundled under the old key, with `state`.
fn installed_by_an_older_build(data: &Path, state: &PluginState) {
    state.write(data).expect("state");
    let shipped = shipped(OLD_KEY_ID, OLD);
    let installed = sync_with(data, shipped.path(), roots(OLD_KEY_ID, OLD)).expect("sync");
    assert_eq!(installed.len(), 1);
    assert_eq!(installed_key(data), key_of(OLD));
}

#[test]
fn a_bundled_version_under_a_key_no_longer_trusted_is_replaced_and_stays_off() {
    let data = tempfile::tempdir().expect("temp dir");
    let mut state = PluginState::default();
    state.disabled.insert(PLUGIN_ID.to_owned());
    installed_by_an_older_build(data.path(), &state);

    // This build trusts only its release key: the same version is put in its place.
    let current = roots(NEW_KEY_ID, NEW);
    let bundled = shipped(NEW_KEY_ID, NEW);
    let replaced = sync_with(data.path(), bundled.path(), &current).expect("sync");
    assert_eq!(replaced, [(PLUGIN_ID.to_owned(), "1.0.0".to_owned())]);
    assert_eq!(installed_key(data.path()), key_of(NEW));
    let verified = entry(version_dir(data.path()), &current().expect("verifier"));
    assert_eq!(verified.refused, None);
    assert!(verified.bundled && verified.signed);

    // The switch is the state's, by id: still off.
    let after = PluginState::read(data.path()).expect("state");
    assert_eq!(after, state);
    assert!(!after.enabled(PLUGIN_ID));
    // Nothing is left beside it, and the next start changes nothing.
    let folders = std::fs::read_dir(plugins_root(data.path()).join(PLUGIN_ID))
        .expect("read")
        .count();
    assert_eq!(folders, 1, "no staging or replaced folder is left");
    assert!(
        sync_with(data.path(), bundled.path(), &current)
            .expect("again")
            .is_empty()
    );
}

#[test]
fn a_bundled_version_that_verifies_or_was_withdrawn_stays() {
    let mut trusted = PluginState::default();
    trusted
        .trusted_keys
        .insert(OLD_KEY_ID.to_owned(), key_of(OLD));
    let mut withdrawn = PluginState::default();
    withdrawn
        .revoked_keys
        .insert(key_fingerprint(&signing_key(OLD).verifying_key()));
    for state in [trusted, withdrawn] {
        let data = tempfile::tempdir().expect("temp dir");
        installed_by_an_older_build(data.path(), &state);
        let bundled = shipped(NEW_KEY_ID, NEW);
        assert!(
            sync_with(data.path(), bundled.path(), roots(NEW_KEY_ID, NEW))
                .expect("sync")
                .is_empty(),
            "{state:?}"
        );
        assert_eq!(installed_key(data.path()), key_of(OLD));
    }

    // A state that does not read replaces nothing either.
    let data = tempfile::tempdir().expect("temp dir");
    installed_by_an_older_build(data.path(), &PluginState::default());
    std::fs::write(
        plugins_root(data.path()).join(crate::installed::STATE_FILE),
        b"{ broken",
    )
    .expect("write");
    let bundled = shipped(NEW_KEY_ID, NEW);
    assert!(
        sync_with(data.path(), bundled.path(), roots(NEW_KEY_ID, NEW))
            .expect("sync")
            .is_empty()
    );
    assert_eq!(installed_key(data.path()), key_of(OLD));
}

#[test]
fn a_version_a_person_installed_is_never_replaced() {
    let data = tempfile::tempdir().expect("temp dir");
    let installer = PluginInstaller::new(
        plugins_root(data.path()),
        roots(OLD_KEY_ID, OLD)().expect("verifier"),
    );
    let package = viewer_package(PLUGIN_ID, "1.0.0", OLD_KEY_ID, &signing_key(OLD), true);
    installer.install_bytes(&package).expect("install");

    let bundled = shipped(NEW_KEY_ID, NEW);
    assert!(
        sync_with(data.path(), bundled.path(), roots(NEW_KEY_ID, NEW))
            .expect("sync")
            .is_empty()
    );
    assert_eq!(installed_key(data.path()), key_of(OLD));
    assert!(!version_dir(data.path()).join(BUNDLED_MARKER).exists());
}
