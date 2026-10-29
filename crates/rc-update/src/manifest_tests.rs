//! What a manipulated, replayed or misplaced manifest does: it is refused, with the code that says
//! why (RC-100-04). Every test signs with a throw-away key against a store of its own.

use chrono::{DateTime, Duration, Utc};

use super::*;
use crate::sign::{SigningKey, StaleError, TrustStore, sign_document};

const KEY_ID: &str = "rcommander-update-v1";

pub(crate) fn key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}

pub(crate) fn trust_for(key: &SigningKey) -> TrustStore {
    let trust = TrustStore::new();
    trust
        .trust(KEY_ID.to_owned(), key.verifying_key())
        .expect("trust");
    trust
}

pub(crate) fn at(days: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_790_000_000, 0).expect("timestamp") + Duration::days(days)
}

pub(crate) const PREFIX: &str = "https://github.com/degoya/rCommander/releases/download/";

pub(crate) fn artifact(platform: &str, arch: &str, kind: &str) -> Artifact {
    Artifact {
        platform: platform.to_owned(),
        arch: arch.to_owned(),
        kind: kind.to_owned(),
        url: format!("{PREFIX}v1.8.0/rcommander-1.8.0-{platform}-{arch}.{kind}"),
        sha256: "ab".repeat(32),
        size: 1024,
    }
}

pub(crate) fn manifest(channel: Channel, version: &str, sequence: u64) -> UpdateManifest {
    UpdateManifest {
        schema_version: UPDATE_MANIFEST_SCHEMA_VERSION,
        sequence,
        issued_at: at(0),
        not_after: at(180),
        channel,
        version: version.to_owned(),
        released_at: at(0),
        notes: "- Update check".to_owned(),
        artifacts: vec![
            artifact("windows", "x86_64", kind::ZIP),
            artifact("linux", "x86_64", kind::TAR_GZ),
        ],
        schema_change: None,
    }
}

pub(crate) fn signed(manifest: &UpdateManifest) -> Vec<u8> {
    sign(KEY_ID, &key(), manifest).expect("sign")
}

fn verified(bytes: &[u8], channel: Channel) -> Result<UpdateManifest, UpdateError> {
    verify_with(bytes, &trust_for(&key()), channel, None, at(1))
}

#[test]
fn a_signed_manifest_verifies_and_round_trips() {
    let original = manifest(Channel::Stable, "1.8.0", 5);
    let read = verified(&signed(&original), Channel::Stable).expect("verify");
    assert_eq!(read.version, "1.8.0");
    assert_eq!(read.sequence, 5);
    assert_eq!(read.artifacts, original.artifacts);
}

/// A forged signature (same id, other key) is tampering; a foreign key id is untrusted.
#[test]
fn a_forged_and_a_foreign_signature_are_refused_with_their_codes() {
    let bytes = sign(
        KEY_ID,
        &SigningKey::from_bytes(&[9; 32]),
        &manifest(Channel::Stable, "1.8.0", 1),
    )
    .expect("sign");
    let error = verified(&bytes, Channel::Stable).expect_err("forged");
    assert_eq!(error.code(), "update.bad_signature", "{error}");
    let foreign = sign(
        "someone-else-v1",
        &SigningKey::from_bytes(&[9; 32]),
        &manifest(Channel::Stable, "1.8.0", 1),
    )
    .expect("sign");
    let error = verified(&foreign, Channel::Stable).expect_err("foreign");
    assert_eq!(error.code(), "update.untrusted", "{error}");
}

#[test]
fn a_tampered_manifest_is_refused() {
    let bytes = signed(&manifest(Channel::Stable, "1.8.0", 1));
    let mut document: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    document["payload"]["artifacts"][0]["url"] =
        serde_json::Value::String("https://evil.example/rcommander.zip".to_owned());
    let tampered = serde_json::to_vec(&document).expect("encode");
    let error = verified(&tampered, Channel::Stable).expect_err("tampered");
    assert_eq!(error.code(), "update.bad_signature");
}

/// The domain separator: the right key over another document kind is no update manifest.
#[test]
fn a_signature_under_another_domain_does_not_verify() {
    let payload = manifest(Channel::Stable, "1.8.0", 1);
    let document =
        sign_document("rcommander.plugin-index.v1", KEY_ID, &key(), &payload).expect("sign");
    let bytes = serde_json::to_vec(&document).expect("encode");
    let error = verified(&bytes, Channel::Stable).expect_err("wrong domain");
    assert_eq!(error.code(), "update.bad_signature");
}

#[test]
fn a_manifest_below_the_floor_is_a_replay_and_one_at_it_is_the_same_manifest() {
    let bytes = signed(&manifest(Channel::Stable, "1.8.0", 5));
    let trust = trust_for(&key());
    let error = verify_with(&bytes, &trust, Channel::Stable, Some(6), at(1)).expect_err("replay");
    assert!(
        matches!(
            error,
            UpdateError::Stale(StaleError::Replayed { saw: 5, known: 6 })
        ),
        "{error}"
    );
    assert_eq!(error.code(), "update.stale");
    verify_with(&bytes, &trust, Channel::Stable, Some(5), at(1)).expect("the accepted manifest");
}

#[test]
fn an_expired_and_a_future_manifest_are_stale() {
    let bytes = signed(&manifest(Channel::Stable, "1.8.0", 1));
    let trust = trust_for(&key());
    let expired = verify_with(&bytes, &trust, Channel::Stable, None, at(181)).expect_err("expired");
    assert!(matches!(
        expired,
        UpdateError::Stale(StaleError::Expired { .. })
    ));
    let future = verify_with(&bytes, &trust, Channel::Stable, None, at(-7)).expect_err("future");
    assert!(matches!(
        future,
        UpdateError::Stale(StaleError::FromTheFuture { .. })
    ));
    assert_eq!(future.code(), "update.stale");
}

/// A genuine beta manifest at the stable address must not reach a stable installation.
#[test]
fn a_beta_manifest_at_the_stable_address_is_the_wrong_channel() {
    let bytes = signed(&manifest(Channel::Beta, "1.8.0-beta.2", 1));
    let error = verified(&bytes, Channel::Stable).expect_err("wrong channel");
    assert_eq!(error.code(), "update.wrong_channel");
}

#[test]
fn a_pre_release_in_the_stable_manifest_is_refused_at_signing_and_at_verification() {
    assert!(
        sign(
            KEY_ID,
            &key(),
            &manifest(Channel::Stable, "1.8.0-beta.1", 1)
        )
        .is_err()
    );
    assert!(sign(KEY_ID, &key(), &manifest(Channel::Beta, "1.8.0", 1)).is_err());
    // Signed around `sign`'s check, as a careless publisher could.
    let document = sign_document(
        UPDATE_MANIFEST_DOMAIN,
        KEY_ID,
        &key(),
        &manifest(Channel::Stable, "1.8.0-beta.1", 1),
    )
    .expect("sign");
    let bytes = serde_json::to_vec(&document).expect("encode");
    let error = verified(&bytes, Channel::Stable).expect_err("beta on stable");
    assert_eq!(error.code(), "update.invalid");
}

#[test]
fn a_schema_version_one_higher_is_refused_after_the_signature() {
    let mut payload = serde_json::to_value(manifest(Channel::Stable, "1.8.0", 1)).expect("value");
    payload["schema_version"] = serde_json::json!(UPDATE_MANIFEST_SCHEMA_VERSION + 1);
    let document = sign_document(UPDATE_MANIFEST_DOMAIN, KEY_ID, &key(), &payload).expect("sign");
    let bytes = serde_json::to_vec(&document).expect("encode");
    let error = verified(&bytes, Channel::Stable).expect_err("schema");
    assert_eq!(error.code(), "update.schema_version_unsupported");
    // Unsigned, the same document is an untrusted one, not an unknown version.
    let error = verify_with(&bytes, &TrustStore::new(), Channel::Stable, None, at(1))
        .expect_err("unsigned");
    assert_eq!(error.code(), "update.untrusted");
}

#[test]
fn an_unknown_field_verifies_and_a_missing_schema_change_reads_as_true() {
    let mut payload = serde_json::to_value(manifest(Channel::Stable, "1.8.0", 1)).expect("value");
    payload["minimum_backup_version"] = serde_json::json!("1.7.0");
    payload["artifacts"][0]["signature_url"] = serde_json::json!("https://example.test/a.sig");
    assert!(payload.get("schema_change").is_none());
    let document = sign_document(UPDATE_MANIFEST_DOMAIN, KEY_ID, &key(), &payload).expect("sign");
    let bytes = serde_json::to_vec(&document).expect("encode");
    let read = verified(&bytes, Channel::Stable).expect("an additive field verifies");
    assert_eq!(read.artifacts.len(), 2);
    assert!(read.changes_schema());
    let mut quiet = manifest(Channel::Stable, "1.8.0", 1);
    quiet.schema_change = Some(false);
    let read = verified(&signed(&quiet), Channel::Stable).expect("verify");
    assert!(!read.changes_schema());
}

#[test]
fn artifacts_are_held_to_https_a_real_digest_and_one_entry_each() {
    let mut plain_http = manifest(Channel::Stable, "1.8.0", 1);
    plain_http.artifacts[0].url = "http://github.com/a.zip".to_owned();
    assert!(plain_http.validate().is_err());
    let mut short_hash = manifest(Channel::Stable, "1.8.0", 1);
    short_hash.artifacts[0].sha256 = "abc".to_owned();
    assert!(short_hash.validate().is_err());
    let mut empty = manifest(Channel::Stable, "1.8.0", 1);
    empty.artifacts[0].size = 0;
    assert!(empty.validate().is_err());
    let mut twice = manifest(Channel::Stable, "1.8.0", 1);
    twice.artifacts.push(twice.artifacts[0].clone());
    assert!(twice.validate().is_err());
    // A platform this build does not know is listed, never selected.
    let mut later = manifest(Channel::Stable, "1.8.0", 1);
    later.artifacts.push(artifact("freebsd", "riscv64", "pkg"));
    later
        .validate()
        .expect("an unknown platform is not refused");
}

#[test]
fn an_oversized_document_is_refused_before_it_is_parsed() {
    let bytes = vec![b' '; MAX_MANIFEST_BYTES + 1];
    let error = verified(&bytes, Channel::Stable).expect_err("too large");
    assert_eq!(error.code(), "update.too_large");
}

/// The shipped table: with the placeholder no update verifies (fails closed); with the real root
/// a manifest by the test key does not.
#[test]
fn the_compiled_in_root_refuses_a_test_key() {
    let error = verify(
        &signed(&manifest(Channel::Stable, "1.8.0", 1)),
        Channel::Stable,
        None,
        at(1),
    )
    .expect_err("a test key is not the update root");
    let placeholder = EMBEDDED_KEYS
        .iter()
        .all(|entry| entry.public_key == crate::sign::PLACEHOLDER_KEY);
    let expected = if placeholder {
        "update.not_configured"
    } else {
        "update.bad_signature"
    };
    assert_eq!(error.code(), expected, "{error}");
}

#[test]
fn a_build_reads_the_channel_of_its_own_version_until_one_is_chosen() {
    assert_eq!(Channel::of_version("1.1.0-beta.1"), Channel::Beta);
    assert_eq!(Channel::of_version("1.1.0"), Channel::Stable);
    assert_eq!(Channel::of_version("garbage"), Channel::Stable);
}
