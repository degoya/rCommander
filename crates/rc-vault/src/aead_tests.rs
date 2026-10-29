use super::*;
use crate::kdf::KEY_LEN;

fn key(byte: u8) -> VaultKey {
    VaultKey::from_bytes([byte; KEY_LEN])
}

#[test]
fn a_value_opens_with_its_key_and_field() {
    let value = seal(&key(1), b"t/c/1", b"hunter2").expect("seals");
    let plaintext = open(&key(1), b"t/c/1", &value).expect("opens");
    assert_eq!(plaintext.as_slice(), b"hunter2");
}

#[test]
fn the_format_is_version_nonce_ciphertext() {
    let value = seal(&key(1), b"t/c/1", b"hunter2").expect("seals");
    assert_eq!(value[0], FORMAT_V1);
    assert_eq!(value.len(), 1 + NONCE_LEN + b"hunter2".len() + TAG_LEN);
    assert!(!value.windows(7).any(|window| window == b"hunter2"));
}

#[test]
fn the_same_plaintext_seals_differently_each_time() {
    let first = seal(&key(1), b"t/c/1", b"hunter2").expect("seals");
    let second = seal(&key(1), b"t/c/1", b"hunter2").expect("seals");
    assert_ne!(first, second);
    assert_ne!(first[1..=NONCE_LEN], second[1..=NONCE_LEN]);
}

#[test]
fn another_field_or_key_does_not_open_it() {
    let value = seal(&key(1), b"t/c/1", b"hunter2").expect("seals");
    for aad in [&b"t/c/2"[..], &b"t/d/1"[..], &b"u/c/1"[..], &b""[..]] {
        assert!(matches!(
            open(&key(1), aad, &value),
            Err(VaultError::Undecryptable)
        ));
    }
    assert!(matches!(
        open(&key(2), b"t/c/1", &value),
        Err(VaultError::Undecryptable)
    ));
}

#[test]
fn a_changed_byte_is_caught() {
    let value = seal(&key(1), b"t/c/1", b"hunter2").expect("seals");
    for index in 1..value.len() {
        let mut damaged = value.clone();
        damaged[index] ^= 0x01;
        assert!(
            matches!(
                open(&key(1), b"t/c/1", &damaged),
                Err(VaultError::Undecryptable)
            ),
            "byte {index}"
        );
    }
}

#[test]
fn short_or_foreign_values_are_malformed() {
    let value = seal(&key(1), b"t/c/1", b"").expect("seals an empty value");
    assert_eq!(value.len(), 1 + NONCE_LEN + TAG_LEN);
    assert!(open(&key(1), b"t/c/1", &value).expect("opens").is_empty());
    let mut other_format = value.clone();
    other_format[0] = 2;
    for bad in [
        &[][..],
        &[FORMAT_V1][..],
        &value[..value.len() - 1],
        &other_format[..],
    ] {
        assert!(matches!(
            open(&key(1), b"t/c/1", bad),
            Err(VaultError::Malformed)
        ));
    }
}

#[test]
fn the_verifier_round_trips() {
    let value = seal(&key(3), VERIFIER_AAD, VERIFIER_PLAINTEXT).expect("seals");
    assert_eq!(
        open(&key(3), VERIFIER_AAD, &value)
            .expect("opens")
            .as_slice(),
        VERIFIER_PLAINTEXT
    );
}
