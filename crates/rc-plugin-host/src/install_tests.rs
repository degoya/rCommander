//! The acceptance test of RC-100-09 on the installation: atomic (a stop half-way leaves nothing
//! that loads, the next start removes it), a version in use not removable.

use super::*;
use crate::{
    installed::{self, PluginState},
    test_fixtures::PLUGIN_ID,
    test_support::{signing_key, viewer_package},
};

const KEY_ID: &str = "example-release-v1";

/// A data folder whose state trusts the test key, and an installer under it.
fn setup() -> (tempfile::TempDir, PluginInstaller) {
    let data = tempfile::tempdir().expect("temp dir");
    let mut state = PluginState::default();
    state
        .trusted_keys
        .insert(KEY_ID.to_owned(), crate::public_key_base64(&signing_key(7)));
    state.write(data.path()).expect("state");
    let installer = PluginInstaller::new(
        installed::plugins_root(data.path()),
        state.verifier().expect("verifier"),
    );
    (data, installer)
}

fn package(version: &str) -> Vec<u8> {
    viewer_package(PLUGIN_ID, version, KEY_ID, &signing_key(7), true)
}

#[test]
fn an_install_lands_in_its_version_folder_and_lists() {
    let (data, installer) = setup();
    let done = installer.install_bytes(&package("1.0.0")).expect("install");
    assert!(done.path.ends_with(format!("{PLUGIN_ID}/1.0.0")));
    assert!(done.path.join("component.wasm").is_file());
    let listed = installed::list(data.path()).expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].manifest.version, "1.0.0");
    assert!(listed[0].enabled && listed[0].signed && !listed[0].bundled);
    let again = installer
        .install_bytes(&package("1.0.0"))
        .expect_err("installed already");
    assert_eq!(again.code(), "plugin_version_installed");
}

#[test]
fn a_stop_before_the_rename_leaves_nothing_that_loads_and_the_start_removes_it() {
    let (data, installer) = setup();
    let verified = installer
        .verifier()
        .verify_bytes(&package("1.0.0"))
        .expect("verifies");
    // The install stops after writing its staging folder: the rename never happens.
    let staging = installer.stage(&verified, false).expect("staged");
    assert!(staging.join("component.wasm").is_file());
    assert!(installed::list(data.path()).expect("list").is_empty());
    let state = PluginState::read(data.path()).expect("state");
    assert!(
        installed::inventory(data.path(), &state)
            .expect("inventory")
            .is_empty()
    );

    let sweep = installed::sweep_at_start(data.path(), &state);
    assert_eq!(sweep.stagings_removed, 1);
    assert!(!staging.exists());
    // The interrupted version installs cleanly afterwards.
    installer
        .install_verified(&verified, false)
        .expect("install");
    assert_eq!(installed::list(data.path()).expect("list").len(), 1);
}

#[test]
fn a_failed_rename_leaves_no_staging_folder() {
    let (_data, installer) = setup();
    let verified = installer
        .verifier()
        .verify_bytes(&package("1.0.0"))
        .expect("verifies");
    let staging = installer.stage(&verified, false).expect("staged");
    // A target inside a folder that does not exist cannot be renamed to.
    let target = staging.with_file_name("missing").join("1.0.0");
    assert!(PluginInstaller::promote(&staging, &target).is_err());
    assert!(!staging.exists());
}

#[test]
fn a_version_in_use_is_not_removable_until_it_is_let_go() {
    let (data, installer) = setup();
    installer.install_bytes(&package("1.0.0")).expect("install");
    installer.install_bytes(&package("1.1.0")).expect("install");
    let listed = installed::list(data.path()).expect("list");
    assert_eq!(listed[0].manifest.version, "1.1.0", "the newest runs");
    let held = installed::hold(&listed[0].version_dir);
    let refused = installer
        .remove_version(PLUGIN_ID, "1.1.0")
        .expect_err("in use");
    assert_eq!(crate::refusal_code(&refused), "plugin_version_in_use");
    assert!(installed::in_use(&listed[0].version_dir));
    drop(held);
    assert!(!installed::in_use(&listed[0].version_dir));
    assert!(
        installer
            .remove_version(PLUGIN_ID, "1.1.0")
            .expect("removed")
    );
    assert!(
        !installer
            .remove_version(PLUGIN_ID, "1.1.0")
            .expect("nothing left")
    );
    let listed = installed::list(data.path()).expect("list");
    assert_eq!(listed[0].manifest.version, "1.0.0");
}

#[test]
fn segments_that_could_leave_the_plugin_folder_are_refused() {
    let (_data, installer) = setup();
    for (id, version) in [
        ("..", "1.0.0"),
        (PLUGIN_ID, ".."),
        (PLUGIN_ID, "a/b"),
        ("", "1"),
    ] {
        assert!(
            installer.version_dir(id, version).is_err(),
            "{id} {version}"
        );
    }
    let digest = installer
        .installed_package_digest(PLUGIN_ID, "9.9.9")
        .expect("read");
    assert_eq!(digest, None);
}
