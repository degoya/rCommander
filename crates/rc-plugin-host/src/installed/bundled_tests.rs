use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Result;

use super::{
    BUNDLED_MARKER, bundled_dir_of,
    seen::{Seen, context},
    sync_with,
};
use crate::{
    PluginInstaller, PluginState, PluginVerifier,
    installed::{entry, plugins_root},
    key_fingerprint,
    package::MANIFEST_MEMBER,
    public_key_base64,
    test_fixtures::PLUGIN_ID,
    test_support::{signing_key, viewer_component, viewer_package, viewer_package_unchecked},
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

/// Version 1.0.0 rebuilt: the viewer's component with one more custom section, signed by this
/// build's key as [`shipped`] signs it - other bytes, the same plugin and version. With the
/// component, as the folder of bundled packages holds it.
fn rebuilt() -> (tempfile::TempDir, Vec<u8>) {
    let mut component = viewer_component();
    // A custom section: id 0, 9 bytes - the name's length, "rebuild", one byte of payload.
    component.extend_from_slice(&[0x00, 0x09, 0x07]);
    component.extend_from_slice(b"rebuild2");
    let folder = tempfile::tempdir().expect("bundled");
    let package = viewer_package_unchecked(
        PLUGIN_ID,
        "1.0.0",
        NEW_KEY_ID,
        &signing_key(NEW),
        &component,
    );
    std::fs::write(folder.path().join("csv.rcplug"), package).expect("write");
    (folder, component)
}

#[test]
fn a_bundled_version_rebuilt_under_the_same_key_is_replaced_and_stays_off() {
    let data = tempfile::tempdir().expect("temp dir");
    let mut state = PluginState::default();
    state.disabled.insert(PLUGIN_ID.to_owned());
    state.write(data.path()).expect("state");
    let current = roots(NEW_KEY_ID, NEW);
    let first = shipped(NEW_KEY_ID, NEW);
    assert_eq!(
        sync_with(data.path(), first.path(), &current)
            .expect("sync")
            .len(),
        1
    );
    // The same build again replaces nothing.
    assert!(
        sync_with(data.path(), first.path(), &current)
            .expect("same build")
            .is_empty()
    );

    // The next release ships 1.0.0 rebuilt: it takes the installed one's place.
    let (bundled, component) = rebuilt();
    let replaced = sync_with(data.path(), bundled.path(), &current).expect("sync");
    assert_eq!(replaced, [(PLUGIN_ID.to_owned(), "1.0.0".to_owned())]);
    let installed =
        std::fs::read(version_dir(data.path()).join(crate::COMPONENT_MEMBER)).expect("component");
    assert_eq!(installed, component);
    assert!(version_dir(data.path()).join(BUNDLED_MARKER).is_file());
    let verified = entry(version_dir(data.path()), &current().expect("verifier"));
    assert_eq!(verified.refused, None);
    // What the start compiled is installed now, so the compile cache's prune keeps its entry.
    assert_eq!(
        crate::installed::component_digests(data.path()).expect("digests"),
        BTreeSet::from([crate::sha256_hex(&component)])
    );

    // The switch is the state's, by id: still off; nothing is left beside it, and the next
    // start changes nothing.
    let after = PluginState::read(data.path()).expect("state");
    assert_eq!(after, state);
    assert!(!after.enabled(PLUGIN_ID));
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

/// Whether the record of the judged packages holds `file`, under the state in `data`.
fn recorded(data: &Path, file: &Path) -> bool {
    let state = PluginState::read(data).expect("state");
    Seen::read(data, &context("test roots", &state)).holds(file)
}

#[test]
fn an_unchanged_bundled_package_is_not_read_again() {
    let data = tempfile::tempdir().expect("temp dir");
    PluginState::default().write(data.path()).expect("state");
    let current = roots(NEW_KEY_ID, NEW);
    let bundled = shipped(NEW_KEY_ID, NEW);
    let file = bundled.path().join("csv.rcplug");
    assert_eq!(
        sync_with(data.path(), bundled.path(), &current)
            .expect("sync")
            .len(),
        1
    );
    assert!(recorded(data.path(), &file));

    // Other bytes of the same size under the same time: a start that read them would refuse
    // them and drop their record.
    let metadata = std::fs::metadata(&file).expect("metadata");
    let modified = metadata.modified().expect("time");
    let size = usize::try_from(metadata.len()).expect("small");
    std::fs::write(&file, vec![0_u8; size]).expect("write");
    let set_time = |time| {
        std::fs::File::options()
            .write(true)
            .open(&file)
            .and_then(|opened| opened.set_modified(time))
            .expect("time");
    };
    set_time(modified);
    assert!(
        sync_with(data.path(), bundled.path(), &current)
            .expect("again")
            .is_empty()
    );
    assert!(recorded(data.path(), &file), "passed over unread");

    // Another time: read, refused, no longer recorded; the installed version stays.
    set_time(modified + Duration::from_secs(2));
    assert!(
        sync_with(data.path(), bundled.path(), &current)
            .expect("checked")
            .is_empty()
    );
    assert!(!recorded(data.path(), &file));
    assert!(version_dir(data.path()).is_dir());
}

#[test]
fn a_recorded_package_whose_version_is_gone_is_installed_again() {
    let data = tempfile::tempdir().expect("temp dir");
    PluginState::default().write(data.path()).expect("state");
    let current = roots(NEW_KEY_ID, NEW);
    let bundled = shipped(NEW_KEY_ID, NEW);
    assert_eq!(
        sync_with(data.path(), bundled.path(), &current)
            .expect("sync")
            .len(),
        1
    );
    std::fs::remove_dir_all(version_dir(data.path())).expect("remove");
    let again = sync_with(data.path(), bundled.path(), &current).expect("again");
    assert_eq!(again, [(PLUGIN_ID.to_owned(), "1.0.0".to_owned())]);
    assert!(version_dir(data.path()).join(BUNDLED_MARKER).is_file());
}

/// RC-110-55: the shipped packages by id and version, read whatever key signed them; a file that
/// is no package and one of another extension are passed over.
#[test]
fn the_shipped_versions_are_named_by_their_manifests() {
    let bundled = shipped(OLD_KEY_ID, OLD);
    std::fs::write(bundled.path().join("broken.rcplug"), b"not a zip").expect("write");
    let other = viewer_package(PLUGIN_ID, "2.0.0", NEW_KEY_ID, &signing_key(NEW), true);
    std::fs::write(bundled.path().join("csv.zip"), other).expect("write");
    assert_eq!(
        super::bundled_versions(bundled.path()),
        BTreeSet::from([(PLUGIN_ID.to_owned(), "1.0.0".to_owned())])
    );
    assert!(super::bundled_versions(&bundled.path().join("missing")).is_empty());
}
