use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};

use super::*;

const DOMAIN: &str = "rcommander.test-document.v1";

#[derive(Debug, Deserialize, PartialEq, Serialize)]
struct Payload {
    channel: String,
    version: String,
}

fn payload() -> Payload {
    Payload {
        channel: "stable".to_owned(),
        version: "1.0.0".to_owned(),
    }
}

fn signed(key: &SigningKey, key_id: &str) -> (SignedDocument, TrustStore) {
    let document = sign_document(DOMAIN, key_id, key, &payload()).expect("sign");
    let trust = TrustStore::new();
    trust
        .trust(key_id.to_owned(), key.verifying_key())
        .expect("trust");
    (document, trust)
}

#[test]
fn a_signed_document_round_trips_through_json() {
    let key = SigningKey::from_bytes(&[5; 32]);
    let (document, trust) = signed(&key, "release");
    let encoded = serde_json::to_vec(&document).expect("encode");
    let parsed = SignedDocument::parse(&encoded).expect("parse");
    let read: Payload = parsed.verify(DOMAIN, &trust).expect("verify");
    assert_eq!(read, payload());
}

/// The whole point of the length-prefixed domain field.
#[test]
fn a_signature_does_not_carry_over_to_another_document_kind() {
    let key = SigningKey::from_bytes(&[5; 32]);
    let (document, trust) = signed(&key, "release");
    let result: Result<Payload, _> = document.verify("rcommander.plugin-index.v1", &trust);
    assert!(matches!(result, Err(VerifyError::BadSignature { .. })));
}

#[test]
fn tampering_with_the_payload_is_detected() {
    let key = SigningKey::from_bytes(&[5; 32]);
    let (document, trust) = signed(&key, "release");
    let mut encoded: serde_json::Value =
        serde_json::from_slice(&serde_json::to_vec(&document).expect("encode")).expect("json");
    encoded["payload"]["version"] = serde_json::Value::String("9.9.9".to_owned());
    let tampered =
        SignedDocument::parse(&serde_json::to_vec(&encoded).expect("encode")).expect("parse");
    let result: Result<Payload, _> = tampered.verify(DOMAIN, &trust);
    assert!(matches!(result, Err(VerifyError::BadSignature { .. })));
}

#[test]
fn an_unknown_key_is_reported_separately_from_a_broken_signature() {
    let key = SigningKey::from_bytes(&[5; 32]);
    let (document, _) = signed(&key, "release");
    let result: Result<Payload, _> = document.verify(DOMAIN, &TrustStore::new());
    assert!(matches!(result, Err(VerifyError::UntrustedKey { .. })));
}

#[test]
fn a_revoked_digest_is_refused_before_the_signature_is_looked_at() {
    let key = SigningKey::from_bytes(&[5; 32]);
    let (document, trust) = signed(&key, "release");
    trust
        .revoke_digest(document.digest(DOMAIN))
        .expect("revoke");
    let result: Result<Payload, _> = document.verify(DOMAIN, &trust);
    assert!(matches!(result, Err(VerifyError::Revoked)));
}

#[test]
fn a_signature_in_an_unknown_algorithm_does_not_count() {
    let key = SigningKey::from_bytes(&[5; 32]);
    let (mut document, trust) = signed(&key, "release");
    document.signatures[0].algorithm = "rsa".to_owned();
    let result: Result<Payload, _> = document.verify(DOMAIN, &trust);
    assert!(matches!(result, Err(VerifyError::UntrustedKey { .. })));
}
