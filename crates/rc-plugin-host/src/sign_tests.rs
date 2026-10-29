use ed25519_dalek::SigningKey;

use super::*;

#[test]
fn a_detached_signature_verifies_and_another_digest_or_key_does_not() {
    let key = SigningKey::from_bytes(&[5; 32]);
    let other = SigningKey::from_bytes(&[6; 32]);
    let signature = sign_detached(&key, &[1; 32]);
    verify_detached(&key.verifying_key(), &[1; 32], &signature).expect("verifies");
    verify_detached_bytes(&key.verifying_key(), &[1; 32], signature.as_bytes()).expect("bytes");
    assert!(verify_detached(&key.verifying_key(), &[2; 32], &signature).is_err());
    assert!(verify_detached(&other.verifying_key(), &[1; 32], &signature).is_err());
    assert!(verify_detached(&key.verifying_key(), &[1; 32], "not base64!").is_err());
}
