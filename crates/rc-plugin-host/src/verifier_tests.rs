//! The acceptance tests *Signatur* and *Undeklarierter Import* of RC-100-09 at the verifier: a
//! changed byte refused, an unknown key a question, a revoked key or digest refused, an import
//! the manifest does not grant refused when installed.

use super::*;
use crate::{
    package::SIGNATURE_MEMBER,
    test_fixtures::{PLUGIN_ID, signed_by_hand, tampered},
    test_support::{
        component_importing_http, signing_key, viewer_component, viewer_manifest, viewer_package,
    },
};

const KEY_ID: &str = "example-release-v1";

fn trusting(seed: u8, development_mode: bool) -> PluginVerifier {
    let verifier = PluginVerifier::new(development_mode);
    verifier
        .trust_key(KEY_ID.to_owned(), signing_key(seed).verifying_key())
        .expect("trust");
    verifier
}

fn package() -> Vec<u8> {
    viewer_package(PLUGIN_ID, "1.0.0", KEY_ID, &signing_key(7), true)
}

fn code(result: Result<VerifiedPackage, VerifyError>) -> &'static str {
    result.expect_err("refused").code()
}

#[test]
fn a_package_by_a_trusted_key_verifies_with_its_members() {
    let package = trusting(7, false)
        .verify_bytes(&package())
        .expect("verifies");
    assert_eq!(package.manifest.id, PLUGIN_ID);
    assert!(package.signature.is_some());
    assert_eq!(package.component, viewer_component());
}

#[test]
fn one_changed_byte_in_the_component_is_refused() {
    let changed = tampered(&package(), "component.wasm", |bytes| {
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
    });
    assert_eq!(
        code(trusting(7, false).verify_bytes(&changed)),
        "plugin_signature_invalid"
    );
    // The same for a byte of the manifest, and for a signature that is not the package's.
    let manifest = tampered(&package(), "manifest.toml", |bytes| bytes.push(b'\n'));
    assert_eq!(
        code(trusting(7, false).verify_bytes(&manifest)),
        "plugin_signature_invalid"
    );
}

#[test]
fn an_unknown_key_is_a_question_and_a_known_id_with_another_key_a_refusal() {
    let answer = PluginVerifier::new(false).verify_bytes(&package());
    let Err(VerifyError::UntrustedKey {
        key_id,
        fingerprint,
        ..
    }) = answer
    else {
        panic!("an unknown key is a trust decision, got {answer:?}");
    };
    assert_eq!(key_id, KEY_ID);
    assert_eq!(
        fingerprint,
        key_fingerprint(&signing_key(7).verifying_key())
    );
    assert_eq!(
        code(trusting(8, false).verify_bytes(&package())),
        "plugin_key_mismatch"
    );
}

#[test]
fn a_revoked_key_or_a_withdrawn_digest_is_refused() {
    let verifier = trusting(7, false);
    let package = package();
    let digest = verifier.verify_bytes(&package).expect("verifies").digest();

    verifier.revoked_digests().insert(digest).expect("withdraw");
    assert_eq!(
        code(verifier.verify_bytes(&package)),
        "plugin_digest_revoked"
    );
    verifier.revoked_digests().remove(&digest).expect("restore");

    let fingerprint = key_fingerprint(&signing_key(7).verifying_key());
    verifier
        .revoked_keys()
        .insert(&fingerprint)
        .expect("revoke");
    assert_eq!(code(verifier.verify_bytes(&package)), "plugin_key_revoked");
    // Not even asked about when nobody trusts it.
    let stranger = PluginVerifier::new(false);
    stranger
        .revoked_keys()
        .insert(&fingerprint)
        .expect("revoke");
    assert_eq!(code(stranger.verify_bytes(&package)), "plugin_key_revoked");
}

#[test]
fn an_import_the_manifest_does_not_grant_is_refused_when_installed() {
    let key = signing_key(7);
    let manifest = viewer_manifest(PLUGIN_ID, "1.0.0", KEY_ID, &key);
    let package = signed_by_hand(&manifest, &component_importing_http(), &key);
    assert_eq!(
        code(trusting(7, false).verify_bytes(&package)),
        "plugin_denied"
    );
}

#[test]
fn unsigned_needs_development_mode_and_the_signature_decides_before_the_component() {
    let unsigned = viewer_package(PLUGIN_ID, "1.0.0", KEY_ID, &signing_key(7), false);
    assert_eq!(
        code(trusting(7, false).verify_bytes(&unsigned)),
        "plugin_unsigned"
    );
    let package = trusting(7, true)
        .verify_bytes(&unsigned)
        .expect("development mode");
    assert!(package.signature.is_none());

    // A stripped signature on a component that is none: refused for what is missing.
    let garbage = tampered(&unsigned, "component.wasm", |bytes| {
        *bytes = b"not a component".to_vec();
    });
    assert_eq!(
        code(trusting(7, false).verify_bytes(&garbage)),
        "plugin_unsigned"
    );
    assert!(
        !crate::test_fixtures::members(&unsigned)
            .iter()
            .any(|(name, _)| name == SIGNATURE_MEMBER)
    );
}

#[test]
fn a_member_the_format_does_not_know_is_refused() {
    let extra = crate::test_fixtures::zip_package(&[
        ("manifest.toml", b"manifest_version = 1"),
        ("component.wasm", b"\0asm"),
        ("readme.txt", b"hello"),
    ]);
    assert_eq!(
        code(trusting(7, false).verify_bytes(&extra)),
        "plugin_invalid"
    );
    assert_eq!(refusal_code(&anyhow::anyhow!("anything")), "plugin_invalid");
}

/// RC-080-43 (S7): an update of an installed plugin under another key - with a key id of its
/// own, so no mismatch - is refused before its key is a question, trusted or not.
#[test]
fn an_installed_plugin_under_another_key_is_refused() {
    let installed = InstalledKeys::from_entries([(
        PLUGIN_ID.to_owned(),
        key_fingerprint(&signing_key(8).verifying_key()),
    )]);
    let stranger = viewer_package(PLUGIN_ID, "1.1.0", "stranger-v1", &signing_key(7), true);
    let untrusted = PluginVerifier::new(false).with_installed_keys(installed.clone());
    assert_eq!(
        code(untrusted.verify_bytes(&stranger)),
        "plugin_key_changed"
    );
    let trusted = PluginVerifier::new(false);
    trusted
        .trust_key("stranger-v1".to_owned(), signing_key(7).verifying_key())
        .expect("trust");
    let trusted = trusted.with_installed_keys(installed.clone());
    assert_eq!(code(trusted.verify_bytes(&stranger)), "plugin_key_changed");
    // A tampered package says so first.
    let broken = tampered(&stranger, "component.wasm", |bytes| bytes.push(0));
    assert_eq!(
        code(untrusted.verify_bytes(&broken)),
        "plugin_signature_invalid"
    );
    // Unsigned in development mode as much.
    let unsigned = viewer_package(PLUGIN_ID, "1.1.0", "stranger-v1", &signing_key(7), false);
    let development = PluginVerifier::new(true).with_installed_keys(installed.clone());
    assert_eq!(
        code(development.verify_bytes(&unsigned)),
        "plugin_key_changed"
    );
    // The key it was installed under goes on as before, and so does another plugin.
    let update = viewer_package(PLUGIN_ID, "1.1.0", KEY_ID, &signing_key(8), true);
    assert!(
        trusting(8, false)
            .with_installed_keys(installed.clone())
            .verify_bytes(&update)
            .is_ok()
    );
    let other = viewer_package(
        "019d0000-0000-7000-8000-0000000000b2",
        "1.0.0",
        "stranger-v1",
        &signing_key(7),
        true,
    );
    assert!(matches!(
        PluginVerifier::new(false)
            .with_installed_keys(installed)
            .verify_bytes(&other),
        Err(VerifyError::UntrustedKey { .. })
    ));
}
