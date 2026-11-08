//! A PAM system's source (RC-100-38): it lends its secret for a session and takes it back; only
//! a manifest saying `[secrets] checkout = true` of contract 0.5.0 or later is asked to; what it
//! lends is held to the rules of `get`'s value and never written down.

use rc_plugin_api::secrets::{REASON_REQUIRED, SecretField};
use rc_plugin_types::FailureKind;
use secrecy::ExposeSecret as _;

use super::{MAX_REASON_CHARS, SecretsOptions, SecretsPlugin};
use crate::{
    failure::codes,
    manifest::parse_manifest,
    test_fixtures::secrets_manifest_text,
    testbed::{
        TRAP,
        secrets::{Export, LEASE_ID, LENT, SecretsBed, ok_lease},
    },
};

const LENDING: &str = "\n[secrets]\ncheckout = true\n";

fn lending(bed: &SecretsBed) -> SecretsPlugin {
    let manifest = parse_manifest(secrets_manifest_text(None, LENDING).as_bytes())
        .expect("a lending source's manifest");
    SecretsPlugin::new(&manifest, &bed.build(), SecretsOptions::default())
        .expect("a lending source")
}

#[tokio::test]
async fn a_lending_source_lends_and_takes_back() {
    let source = lending(&SecretsBed::new().lending_until(4_102_444_800));
    assert!(source.lends());
    let lease = source
        .checkout("7c17", SecretField::Password, None)
        .await
        .expect("lent");
    assert_eq!(lease.id, LEASE_ID);
    assert_eq!(lease.value.expose_secret(), LENT);
    assert_eq!(lease.expires, Some(4_102_444_800));
    source.checkin(&lease.id).await.expect("taken back");

    let records = format!(
        "{:?}",
        source.execution_log().records(&source.identity().id)
    );
    assert!(
        records.contains("checkout") && records.contains("checkin"),
        "{records}"
    );
    assert!(!records.contains(LENT), "{records}");
    assert!(!format!("{lease:?}").contains(LENT));
}

/// The system's wish for a reason reaches the engine as its code; with one, it lends.
#[tokio::test]
async fn a_reason_is_handed_over_when_the_system_wants_one() {
    let source = lending(&SecretsBed::new().wanting_a_reason());
    let refused = source
        .checkout("7c17", SecretField::Password, None)
        .await
        .expect_err("a reason is wanted");
    assert_eq!(refused.code(), Some(REASON_REQUIRED));
    let lease = source
        .checkout("7c17", SecretField::Password, Some("ticket 4711"))
        .await
        .expect("lent with a reason");
    assert_eq!(lease.value.expose_secret(), LENT);

    for reason in ["", "  ", "two\nlines"] {
        let failure = source
            .checkout("7c17", SecretField::Password, Some(reason))
            .await
            .expect_err("refused before the plugin");
        assert_eq!(
            failure.code(),
            Some(codes::PLUGIN_REQUEST_TOO_LARGE),
            "{reason:?}"
        );
    }
    let long = "x".repeat(MAX_REASON_CHARS + 1);
    assert!(
        source
            .checkout("7c17", SecretField::Password, Some(&long))
            .await
            .is_err()
    );
}

/// A source that does not say it lends is asked neither; a lending manifest needs contract
/// 0.5.0, and its component has to export both functions.
#[tokio::test]
async fn only_a_declared_lender_of_contract_0_5_is_asked() {
    let manifest = parse_manifest(secrets_manifest_text(None, "").as_bytes()).expect("manifest");
    let plain = SecretsPlugin::new(
        &manifest,
        &SecretsBed::new().build(),
        SecretsOptions::default(),
    )
    .expect("a plain source");
    assert!(!plain.lends());
    let failure = plain
        .checkout("7c17", SecretField::Password, None)
        .await
        .expect_err("not a lender");
    assert_eq!(failure.kind, FailureKind::NotSupported);
    assert_eq!(
        plain
            .checkin(LEASE_ID)
            .await
            .expect_err("nothing lent")
            .kind,
        FailureKind::NotSupported
    );

    let old = secrets_manifest_text(None, LENDING).replace(
        &format!("api_version = \"{}\"", rc_plugin_api::API_VERSION),
        "api_version = \"0.4.0\"",
    );
    assert!(
        parse_manifest(old.as_bytes()).is_err(),
        "a 0.4.0 lender is refused"
    );

    let inventory = crate::test_fixtures::inventory_manifest_text(false, LENDING);
    assert!(
        parse_manifest(inventory.as_bytes()).is_err(),
        "only a secret source has a [secrets] section"
    );
}

/// A lease is held to the rules of `get`'s value: an empty value or an empty id fails the one
/// call with a code and no value; a crash in `checkin` fails that call.
#[tokio::test]
async fn a_bad_lease_fails_the_call() {
    for (empty_id, empty_value) in [(false, true), (true, false)] {
        let mut bed = SecretsBed::new();
        let empty = bed.data(b"");
        let id = if empty_id {
            empty
        } else {
            bed.data(LEASE_ID.as_bytes())
        };
        let value = if empty_value {
            empty
        } else {
            bed.data(LENT.as_bytes())
        };
        let source = lending(&bed.body(Export::Checkout, ok_lease(id, value, None)));
        let failure = source
            .checkout("7c17", SecretField::Password, None)
            .await
            .expect_err("refused");
        assert_eq!(failure.code(), Some(codes::PLUGIN_BAD_OUTPUT));
    }
    let crashing = lending(&SecretsBed::new().body(Export::Checkin, TRAP));
    assert!(crashing.checkin(LEASE_ID).await.is_err());
}

/// Contract 0.5.0 is additive: a secret source built against 0.4.0 has no `checkout` and still
/// hands out its value.
#[tokio::test]
async fn a_source_of_contract_0_4_still_runs() {
    let text = secrets_manifest_text(None, "").replace(
        &format!("api_version = \"{}\"", rc_plugin_api::API_VERSION),
        "api_version = \"0.4.0\"",
    );
    let manifest = parse_manifest(text.as_bytes()).expect("a 0.4.0 manifest");
    let source = SecretsPlugin::new(
        &manifest,
        &SecretsBed::new().contract("0.4.0").build(),
        SecretsOptions::default(),
    )
    .expect("a 0.4.0 source");
    assert!(!source.lends());
    let value = source
        .get("7c17", SecretField::Password)
        .await
        .expect("get");
    assert_eq!(value.expose_secret(), crate::testbed::secrets::CANARY);
}
