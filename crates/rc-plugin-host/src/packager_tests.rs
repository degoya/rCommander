use super::*;
use crate::{
    refusal_code,
    test_fixtures::PLUGIN_ID,
    test_support::{component_importing_http, signing_key, viewer_component, viewer_manifest},
};

#[test]
fn a_generated_key_round_trips_through_its_pem_and_signs_a_package_that_verifies() {
    let generated = generate_signing_key().expect("key");
    let reloaded = load_signing_key_pem(&generated.private_pem).expect("PEM round trip");
    assert_eq!(reloaded.to_bytes(), generated.signing_key.to_bytes());
    assert_eq!(public_key_base64(&reloaded), generated.public_base64);
    let manifest = viewer_manifest(PLUGIN_ID, "1.0.0", "release", &reloaded);
    let package = package_plugin(
        manifest.as_bytes(),
        &viewer_component(),
        &[],
        Some(&reloaded),
    )
    .expect("package");
    let trusted = [("release".to_owned(), generated.public_base64.clone())];
    let verified = verify_package(&package, &trusted, false).expect("verifies");
    assert_eq!(verified.manifest.version, "1.0.0");
    assert!(matches!(
        verify_package(&package, &[], false),
        Err(VerifyError::UntrustedKey { .. })
    ));
    assert!(!format!("{generated:?}").contains("PRIVATE"));
}

/// The PEM `openssl genpkey -algorithm ed25519` writes - the RFC 8410 PKCS#8 prefix and the
/// 32-byte seed - as the development key is kept. Built here, so no key block stands in the tree.
#[test]
fn an_openssl_pem_loads() {
    let mut der = vec![
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    der.extend([9_u8; 32]);
    let body = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &der);
    let label = "PRIVATE KEY";
    let pem = format!("-----BEGIN {label}-----\n{body}\n-----END {label}-----\n");
    let key = load_signing_key_pem(&pem).expect("an openssl Ed25519 key");
    assert_eq!(key.to_bytes(), [9_u8; 32]);
}

#[test]
fn an_import_the_manifest_does_not_grant_is_refused_when_packaged() {
    let key = signing_key(7);
    let manifest = viewer_manifest(PLUGIN_ID, "1.0.0", "release", &key);
    let error = package_plugin(
        manifest.as_bytes(),
        &component_importing_http(),
        &[],
        Some(&key),
    )
    .expect_err("refused");
    assert_eq!(refusal_code(&error), "plugin_denied");
}

#[test]
fn the_manifest_key_must_be_the_signing_key_and_locales_are_signed() {
    let key = signing_key(7);
    let other = signing_key(8);
    let manifest = viewer_manifest(PLUGIN_ID, "1.0.0", "release", &other);
    let error = package_plugin(manifest.as_bytes(), &viewer_component(), &[], Some(&key))
        .expect_err("mismatch");
    assert!(error.to_string().contains("public_key"));

    let manifest = viewer_manifest(PLUGIN_ID, "1.0.0", "release", &key);
    let locales = vec![
        (
            "en".to_owned(),
            format!(r#"{{"name":"CSV","codes":{{"{PLUGIN_ID}.oops":"Oops"}}}}"#).into_bytes(),
        ),
        ("de".to_owned(), br#"{"name":"CSV-Tabelle"}"#.to_vec()),
    ];
    let package = package_plugin(
        manifest.as_bytes(),
        &viewer_component(),
        &locales,
        Some(&key),
    )
    .expect("package");
    let trusted = [("release".to_owned(), public_key_base64(&key))];
    let verified = verify_package(&package, &trusted, false).expect("verifies");
    assert_eq!(verified.locales[0].0, "de", "stored sorted");
    let only_german = [locales[1].clone()];
    assert!(
        package_plugin(
            manifest.as_bytes(),
            &viewer_component(),
            &only_german,
            Some(&key)
        )
        .is_err()
    );
}
