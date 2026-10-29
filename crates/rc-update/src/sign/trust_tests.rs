use ed25519_dalek::SigningKey;

use super::*;

fn key(seed: u8) -> VerifyingKey {
    SigningKey::from_bytes(&[seed; 32]).verifying_key()
}

#[test]
fn an_empty_key_id_is_refused() {
    assert!(TrustStore::new().trust("  ".to_owned(), key(1)).is_err());
}

/// Withdrawing one document must not take its author's other documents with it.
#[test]
fn revoking_a_digest_leaves_the_signing_key_trusted() {
    let store = TrustStore::new();
    store.trust("release".to_owned(), key(1)).expect("trust");
    store.revoke_digest([9; 32]).expect("revoke");
    assert!(store.is_revoked_digest(&[9; 32]).expect("read"));
    assert!(!store.is_revoked_digest(&[8; 32]).expect("read"));
    assert!(store.key("release").expect("read").is_some());
}

#[test]
fn fingerprints_are_stable_hex_sha256() {
    let fingerprint = key_fingerprint(&key(3));
    assert_eq!(fingerprint.len(), 64);
    assert_eq!(fingerprint, key_fingerprint(&key(3)));
}

#[test]
fn a_public_key_of_the_wrong_length_is_refused() {
    assert!(decode_public_key("aGVsbG8=").is_err());
}
