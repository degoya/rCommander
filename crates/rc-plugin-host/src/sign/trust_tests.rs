use ed25519_dalek::SigningKey;

use super::*;

fn key(seed: u8) -> VerifyingKey {
    SigningKey::from_bytes(&[seed; 32]).verifying_key()
}

#[test]
fn trust_is_shared_between_clones_and_revoking_says_whether_it_was_trusted() {
    let store = TrustStore::new();
    let clone = store.clone();
    store.trust("release".to_owned(), key(1)).expect("trust");
    assert_eq!(clone.key("release").expect("read"), Some(key(1)));
    assert_eq!(clone.key_ids().expect("ids"), ["release"]);
    assert!(store.revoke("release").expect("revoke"));
    assert!(!store.revoke("release").expect("revoke"));
    assert_eq!(clone.key("release").expect("read"), None);
}

#[test]
fn an_empty_key_id_is_refused() {
    assert!(TrustStore::new().trust("  ".to_owned(), key(1)).is_err());
}

#[test]
fn fingerprints_are_stable_lower_case_hex_and_wrong_lengths_no_key() {
    let fingerprint = key_fingerprint(&key(3));
    assert_eq!(fingerprint.len(), 64);
    assert!(
        fingerprint
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
    assert_eq!(fingerprint, key_fingerprint(&key(3)));
    assert!(decode_public_key("aGVsbG8=").is_err());
}
