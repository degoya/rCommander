//! The preview: what a package is and where its key stands, before anything is installed.

use super::*;
use crate::{
    installed::InstalledKeys,
    parse_package_digest,
    test_fixtures::{PLUGIN_ID, tampered},
    test_support::{signing_key, viewer_package},
};

const KEY_ID: &str = "example-release-v1";

fn package(signed: bool) -> Vec<u8> {
    viewer_package(PLUGIN_ID, "1.2.0", KEY_ID, &signing_key(7), signed)
}

#[test]
fn an_unknown_key_asks_by_its_fingerprint_and_a_trusted_one_does_not() {
    let preview = preview_package(&package(true), &PluginVerifier::new(false)).expect("preview");
    assert_eq!(preview.key, KeyStatus::Untrusted);
    assert_eq!(
        preview.fingerprint.as_deref(),
        Some(key_fingerprint(&signing_key(7).verifying_key()).as_str())
    );
    assert_eq!(
        (preview.name.as_str(), preview.version.as_str()),
        ("CSV table", "1.2.0")
    );
    assert_eq!(preview.permissions.extensions, ["csv", "tsv"]);
    assert_eq!(preview.permissions.outputs, ["text"]);
    assert!(preview.permissions.capabilities.is_empty());
    assert!(preview.installable(false));

    let verifier = PluginVerifier::new(false);
    verifier
        .trust_key(KEY_ID.to_owned(), signing_key(7).verifying_key())
        .expect("trust");
    assert_eq!(
        preview_package(&package(true), &verifier)
            .expect("preview")
            .key,
        KeyStatus::Trusted
    );
    let other = PluginVerifier::new(false);
    other
        .trust_key(KEY_ID.to_owned(), signing_key(8).verifying_key())
        .expect("trust");
    let mismatch = preview_package(&package(true), &other).expect("preview");
    assert_eq!(mismatch.key, KeyStatus::Mismatch);
    assert!(!mismatch.installable(true));
}

#[test]
fn a_revoked_key_and_a_withdrawn_digest_are_shown_and_not_installable() {
    let verifier = PluginVerifier::new(false);
    let fingerprint = key_fingerprint(&signing_key(7).verifying_key());
    verifier
        .revoked_keys()
        .insert(&fingerprint)
        .expect("revoke");
    let preview = preview_package(&package(true), &verifier).expect("preview");
    assert_eq!(preview.key, KeyStatus::Revoked);
    assert!(!preview.installable(false));

    let verifier = PluginVerifier::new(false);
    let digest = parse_package_digest(&preview.package_digest).expect("digest");
    verifier.revoked_digests().insert(digest).expect("withdraw");
    let preview = preview_package(&package(true), &verifier).expect("preview");
    assert!(preview.withdrawn && !preview.installable(false));
}

#[test]
fn unsigned_is_marked_and_a_broken_signature_is_no_package() {
    let unsigned = preview_package(&package(false), &PluginVerifier::new(false)).expect("preview");
    assert_eq!(unsigned.key, KeyStatus::Unsigned);
    assert_eq!(unsigned.fingerprint, None);
    assert!(!unsigned.installable(false) && unsigned.installable(true));

    let changed = tampered(&package(true), "component.wasm", |bytes| bytes.push(0));
    assert!(preview_package(&changed, &PluginVerifier::new(false)).is_err());
}

#[test]
fn an_update_names_what_it_adds() {
    let old = Permissions {
        extensions: vec!["csv".to_owned()],
        ..Permissions::default()
    };
    let new = Permissions {
        extensions: vec!["csv".to_owned(), "tsv".to_owned()],
        capabilities: vec!["net_http".to_owned()],
        ..Permissions::default()
    };
    assert_eq!(old.added_by(&new), ["net_http", ".tsv"]);
    assert!(new.added_by(&old).is_empty());
}

/// A secret source's loopback port is part of what the person confirms, so an update that moves
/// it asks again (RC-100-33).
#[test]
fn the_granted_port_is_shown_and_a_moved_one_is_new() {
    let granted = Permissions::of(&crate::test_fixtures::secrets_manifest(Some(8087)));
    assert_eq!(granted.capabilities, ["net_http localhost:8087"]);
    let moved = Permissions::of(&crate::test_fixtures::secrets_manifest(Some(8088)));
    assert_eq!(granted.added_by(&moved), ["net_http localhost:8088"]);
    assert!(granted.added_by(&granted).is_empty());
}

/// RC-080-43 (S7): the preview of a plugin installed under another key says so, and installs
/// nothing; the key it was installed under previews as before.
#[test]
fn an_installed_plugin_under_another_key_is_shown_as_changed() {
    let installed = InstalledKeys::from_entries([(
        PLUGIN_ID.to_owned(),
        key_fingerprint(&signing_key(8).verifying_key()),
    )]);
    let verifier = PluginVerifier::new(true).with_installed_keys(installed.clone());
    for signed in [true, false] {
        let preview = preview_package(&package(signed), &verifier).expect("preview");
        assert_eq!(preview.key, KeyStatus::Changed, "signed: {signed}");
        assert!(!preview.installable(true));
    }
    let same = viewer_package(PLUGIN_ID, "1.3.0", KEY_ID, &signing_key(8), true);
    let preview = preview_package(&same, &verifier).expect("preview");
    assert_eq!(preview.key, KeyStatus::Untrusted);
}
