//! `list`, `inventory`, the state, the bundled packages and the rollback's roles.

use super::*;
use crate::{
    PluginInstaller, VersionChoice, public_key_base64,
    test_fixtures::PLUGIN_ID,
    test_support::{signing_key, viewer_package},
};

const KEY_ID: &str = "example-release-v1";

fn state_trusting(seed: u8) -> PluginState {
    let mut state = PluginState::default();
    state
        .trusted_keys
        .insert(KEY_ID.to_owned(), public_key_base64(&signing_key(seed)));
    state
}

fn install(data: &Path, state: &PluginState, versions: &[&str]) {
    state.write(data).expect("state");
    let installer = PluginInstaller::new(plugins_root(data), state.verifier().expect("verifier"));
    for version in versions {
        let package = viewer_package(PLUGIN_ID, version, KEY_ID, &signing_key(7), true);
        installer.install_bytes(&package).expect("install");
    }
}

#[test]
fn without_a_state_only_the_roots_are_trusted_and_a_broken_state_runs_nothing() {
    let data = tempfile::tempdir().expect("temp dir");
    install(data.path(), &state_trusting(7), &["1.0.0"]);
    std::fs::remove_file(plugins_root(data.path()).join(STATE_FILE)).expect("remove");
    assert!(
        list(data.path()).expect("list").is_empty(),
        "the key is no root"
    );
    let entries = inventory(data.path(), &PluginState::default()).expect("inventory");
    assert_eq!(entries[0].refused, Some("plugin_key_untrusted"));

    std::fs::write(plugins_root(data.path()).join(STATE_FILE), b"{ broken").expect("write");
    assert!(list(data.path()).is_err());
}

#[test]
fn switches_disable_one_plugin_or_all_and_survive_a_round_trip() {
    let data = tempfile::tempdir().expect("temp dir");
    let mut state = state_trusting(7);
    install(data.path(), &state, &["1.0.0"]);
    state.disabled.insert(PLUGIN_ID.to_owned());
    state.write(data.path()).expect("state");
    assert_eq!(PluginState::read(data.path()).expect("read"), state);
    let listed = list(data.path()).expect("list");
    assert_eq!(listed.len(), 1, "listed, so it can be switched on again");
    assert!(!listed[0].enabled);

    state.disabled.clear();
    state.plugins_enabled = false;
    state.write(data.path()).expect("state");
    assert!(!list(data.path()).expect("list")[0].enabled);
}

#[test]
fn a_rollback_runs_the_previous_version_and_the_inventory_names_every_role() {
    let data = tempfile::tempdir().expect("temp dir");
    let mut state = state_trusting(7);
    install(data.path(), &state, &["1.0.0", "1.1.0", "2.0.0"]);
    assert_eq!(
        list(data.path()).expect("list")[0].manifest.version,
        "2.0.0"
    );

    let previous = crate::previous_version(&["1.0.0", "1.1.0", "2.0.0"], "2.0.0");
    state.choices.insert(
        PLUGIN_ID.to_owned(),
        VersionChoice {
            active: previous.map(str::to_owned),
            staged: None,
        },
    );
    state.write(data.path()).expect("state");
    assert_eq!(
        list(data.path()).expect("list")[0].manifest.version,
        "1.1.0"
    );

    let entries = inventory(data.path(), &state).expect("inventory");
    let roles: Vec<(&str, Option<VersionRole>)> = entries
        .iter()
        .map(|entry| (entry.version.as_str(), entry.role))
        .collect();
    assert_eq!(
        roles,
        [
            ("2.0.0", Some(VersionRole::Retained)),
            ("1.1.0", Some(VersionRole::Default)),
            ("1.0.0", Some(VersionRole::Retained)),
        ]
    );
    assert!(
        entries
            .iter()
            .all(|entry| entry.signed && entry.fingerprint.is_some())
    );
}

#[test]
fn a_withdrawn_version_drops_out_and_the_previous_one_runs() {
    let data = tempfile::tempdir().expect("temp dir");
    let mut state = state_trusting(7);
    install(data.path(), &state, &["1.0.0", "2.0.0"]);
    let entries = inventory(data.path(), &state).expect("inventory");
    let newest = entries[0].digest.clone().expect("digest");
    state.revoked_digests.insert(newest);
    state.write(data.path()).expect("state");
    assert_eq!(
        list(data.path()).expect("list")[0].manifest.version,
        "1.0.0"
    );
    let entries = inventory(data.path(), &state).expect("inventory");
    assert_eq!(entries[0].refused, Some("plugin_digest_revoked"));
    // The start keeps it: someone has to see it.
    let sweep = sweep_at_start(data.path(), &state);
    assert_eq!(
        sweep.kept,
        [(
            PLUGIN_ID.to_owned(),
            "2.0.0".to_owned(),
            "plugin_digest_revoked"
        )]
    );
}

#[test]
fn a_package_copied_into_another_version_folder_does_not_load() {
    let data = tempfile::tempdir().expect("temp dir");
    let state = state_trusting(7);
    install(data.path(), &state, &["1.0.0"]);
    let root = plugins_root(data.path()).join(PLUGIN_ID);
    copy_dir(&root.join("1.0.0"), &root.join("3.0.0"));
    let entries = inventory(data.path(), &state).expect("inventory");
    let copy = entries
        .iter()
        .find(|entry| entry.version == "3.0.0")
        .expect("copy");
    assert_eq!(copy.refused, Some("plugin_invalid"));
    assert_eq!(
        list(data.path()).expect("list")[0].manifest.version,
        "1.0.0"
    );
}

#[test]
fn a_bundled_package_needs_a_root_key_and_installs_once() {
    let data = tempfile::tempdir().expect("temp dir");
    let bundled = tempfile::tempdir().expect("bundled");
    let package = viewer_package(PLUGIN_ID, "1.0.0", KEY_ID, &signing_key(7), true);
    std::fs::write(bundled.path().join("csv.rcplug"), &package).expect("write");
    std::fs::write(bundled.path().join("notes.txt"), b"not a package").expect("write");
    // Signed by a key that is no root: skipped, never a question at the start.
    assert!(
        sync_bundled(data.path(), bundled.path())
            .expect("sync")
            .is_empty()
    );
    assert!(list(data.path()).expect("list").is_empty());

    // Under a root that signed it (the test key standing in), it installs once, marked.
    let state = state_trusting(7);
    state.write(data.path()).expect("state");
    let root = || state.verifier().expect("verifier");
    let installed = bundled::sync_with(data.path(), bundled.path(), root()).expect("sync");
    assert_eq!(installed, [(PLUGIN_ID.to_owned(), "1.0.0".to_owned())]);
    assert!(
        bundled::sync_with(data.path(), bundled.path(), root())
            .expect("again")
            .is_empty()
    );
    let listed = list(data.path()).expect("list");
    assert!(listed[0].bundled && listed[0].enabled);
}

#[test]
fn the_in_use_count_goes_with_the_last_guard() {
    let dir = Path::new("/nowhere/plugin/1.0.0");
    let first = hold(dir);
    let second = hold(dir);
    drop(first);
    assert!(in_use(dir));
    drop(second);
    assert!(!in_use(dir));
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create");
    for entry in std::fs::read_dir(from).expect("read") {
        let entry = entry.expect("entry");
        if entry.file_type().expect("type").is_file() {
            std::fs::copy(entry.path(), to.join(entry.file_name())).expect("copy");
        }
    }
}
