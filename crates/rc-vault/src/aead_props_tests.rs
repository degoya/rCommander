//! Properties of the value envelope `v1 | nonce | ciphertext` (RC-110-70): for any key, field and
//! content it opens to what was sealed, and to nothing else when a byte, the field or the key
//! differs; no input makes [`open`] panic.

use proptest::prelude::*;

use super::{FORMAT_V1, NONCE_LEN, TAG_LEN, open, seal};
use crate::{
    VaultError,
    kdf::{KEY_LEN, VaultKey},
};

fn sealed(key: [u8; KEY_LEN], aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
    seal(&VaultKey::from_bytes(key), aad, plaintext).expect("a 32-byte key seals")
}

proptest! {
    /// What is sealed opens again with its key and field, in the documented layout.
    #[test]
    fn a_value_round_trips(
        key in any::<[u8; KEY_LEN]>(),
        aad in prop::collection::vec(any::<u8>(), 0..64),
        plaintext in prop::collection::vec(any::<u8>(), 0..512),
    ) {
        let value = sealed(key, &aad, &plaintext);
        prop_assert_eq!(value.len(), 1 + NONCE_LEN + plaintext.len() + TAG_LEN);
        prop_assert_eq!(value[0], FORMAT_V1);
        let opened = open(&VaultKey::from_bytes(key), &aad, &value).expect("opens");
        prop_assert_eq!(opened.as_slice(), plaintext.as_slice());
    }

    /// One changed bit anywhere behind the format byte, another field or another key: the tag
    /// fails.
    #[test]
    fn a_changed_bit_field_or_key_does_not_open(
        key in any::<[u8; KEY_LEN]>(),
        aad in prop::collection::vec(any::<u8>(), 0..64),
        plaintext in prop::collection::vec(any::<u8>(), 0..256),
        at in any::<prop::sample::Index>(),
        bit in 0_u8..8,
        other_aad in prop::collection::vec(any::<u8>(), 0..64),
        other_key in any::<[u8; KEY_LEN]>(),
    ) {
        let value = sealed(key, &aad, &plaintext);
        let mut damaged = value.clone();
        damaged[1 + at.index(value.len() - 1)] ^= 1 << bit;
        prop_assert!(matches!(
            open(&VaultKey::from_bytes(key), &aad, &damaged),
            Err(VaultError::Undecryptable)
        ));
        if other_aad != aad {
            prop_assert!(matches!(
                open(&VaultKey::from_bytes(key), &other_aad, &value),
                Err(VaultError::Undecryptable)
            ));
        }
        if other_key != key {
            prop_assert!(matches!(
                open(&VaultKey::from_bytes(other_key), &aad, &value),
                Err(VaultError::Undecryptable)
            ));
        }
    }

    /// A value cut short is malformed below the smallest envelope, undecryptable above it.
    #[test]
    fn a_shortened_value_is_refused(
        key in any::<[u8; KEY_LEN]>(),
        plaintext in prop::collection::vec(any::<u8>(), 0..128),
        keep in any::<prop::sample::Index>(),
    ) {
        let value = sealed(key, b"t/c/1", &plaintext);
        let short = &value[..keep.index(value.len())];
        let result = open(&VaultKey::from_bytes(key), b"t/c/1", short);
        if short.len() < 1 + NONCE_LEN + TAG_LEN {
            prop_assert!(matches!(result, Err(VaultError::Malformed)));
        } else {
            prop_assert!(matches!(result, Err(VaultError::Undecryptable)));
        }
    }

    /// Bytes from anywhere - a damaged database, a hostile import - are refused, never a panic.
    #[test]
    fn foreign_bytes_are_refused(
        key in any::<[u8; KEY_LEN]>(),
        bytes in prop::collection::vec(any::<u8>(), 0..128),
    ) {
        prop_assert!(open(&VaultKey::from_bytes(key), b"t/c/1", &bytes).is_err());
    }
}
