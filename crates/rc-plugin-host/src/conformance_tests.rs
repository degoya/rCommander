//! `check_package` end to end against the test bed: a working viewer passes, a broken package
//! stops at `package`, a viewer that crashes on an empty file fails `viewer_empty_source`.

use super::check_package;
use crate::{
    PluginVerifier, package_plugin,
    test_support::{signing_key, viewer_manifest},
    testbed::{self, Export, Testbed},
};

const ID: &str = "019d0000-0000-7000-8000-0000000000a1";
const KEY_ID: &str = "conformance-test-v1";

fn package(testbed: &Testbed) -> Vec<u8> {
    let key = signing_key(3);
    let manifest = viewer_manifest(ID, "1.0.0", KEY_ID, &key);
    package_plugin(manifest.as_bytes(), &testbed.build(), &[], Some(&key)).expect("a package")
}

fn verifier() -> PluginVerifier {
    let verifier = PluginVerifier::new(false);
    verifier
        .trust_key(KEY_ID.to_owned(), signing_key(3).verifying_key())
        .expect("trust");
    verifier
}

fn ids(report: &super::ConformanceReport) -> Vec<(&'static str, bool)> {
    report
        .checks
        .iter()
        .map(|check| (check.id, check.passed))
        .collect()
}

#[tokio::test]
async fn a_working_viewer_passes_every_check() {
    let report = check_package(&package(&Testbed::new()), &verifier()).await;
    assert!(report.passed, "{report:?}");
    assert_eq!(
        ids(&report),
        [
            ("package", true),
            ("world", true),
            ("viewer_empty_source", true)
        ]
    );
    assert_eq!(report.plugin_id.as_deref(), Some(ID));
    assert_eq!(report.plugin_type.as_deref(), Some("viewer"));
    let json = serde_json::to_value(&report).expect("the report serializes");
    assert_eq!(json["checks"][2]["id"], "viewer_empty_source");
    assert!(json["checks"][0].get("detail").is_none());
}

#[tokio::test]
async fn an_untrusted_or_broken_package_stops_at_package() {
    let report = check_package(&package(&Testbed::new()), &PluginVerifier::new(false)).await;
    assert!(!report.passed);
    assert_eq!(ids(&report), [("package", false)]);
    assert!(report.checks[0].detail.is_some());

    let report = check_package(b"not a zip", &verifier()).await;
    assert_eq!(ids(&report), [("package", false)]);
}

#[tokio::test]
async fn a_viewer_crashing_on_an_empty_file_fails_its_question() {
    for export in [Export::Probe, Export::Open, Export::Text] {
        let crashing = Testbed::new().body(export, testbed::TRAP);
        let report = check_package(&package(&crashing), &verifier()).await;
        assert!(!report.passed, "{export:?}");
        assert_eq!(
            ids(&report),
            [
                ("package", true),
                ("world", true),
                ("viewer_empty_source", false)
            ],
            "{export:?}"
        );
        let detail = report.checks[2].detail.as_deref().unwrap_or_default();
        assert!(detail.contains("plugin_crashed"), "{export:?}: {detail}");
    }
}

/// A secret source's package with its grant of `loopback`, signed by the trusted key.
fn secrets_package(bed: &testbed::secrets::SecretsBed) -> Vec<u8> {
    let key = signing_key(3);
    let manifest = crate::test_fixtures::secrets_manifest_text(Some(8087), "")
        .replace("example-release-v1", KEY_ID)
        .replace(
            crate::test_fixtures::PUBLIC_KEY,
            &crate::public_key_base64(&key),
        );
    package_plugin(manifest.as_bytes(), &bed.build(), &[], Some(&key)).expect("a package")
}

/// A secret source is asked while its service does not answer: one that answers passes, one that
/// crashes on `status` fails `secrets_no_service` (RC-100-33).
#[tokio::test]
async fn a_secret_source_is_asked_without_its_service() {
    use testbed::secrets::{Export, SecretsBed};

    let report = check_package(&secrets_package(&SecretsBed::new()), &verifier()).await;
    assert!(report.passed, "{report:?}");
    assert_eq!(
        ids(&report),
        [
            ("package", true),
            ("world", true),
            ("secrets_no_service", true)
        ]
    );
    assert_eq!(report.plugin_type.as_deref(), Some("secrets"));

    let crashing = SecretsBed::new().body(Export::Status, testbed::TRAP);
    let report = check_package(&secrets_package(&crashing), &verifier()).await;
    assert!(!report.passed);
    assert_eq!(
        ids(&report),
        [
            ("package", true),
            ("world", true),
            ("secrets_no_service", false)
        ]
    );
}
