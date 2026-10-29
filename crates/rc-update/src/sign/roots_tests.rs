use base64::Engine as _;
use ed25519_dalek::SigningKey;

use super::*;
use crate::sign::{VerifyError, sign_document};

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_760_000_000, 0).expect("timestamp")
}

fn instant(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .expect("date")
        .with_timezone(&Utc)
}

fn public(seed: u8) -> &'static str {
    let key = SigningKey::from_bytes(&[seed; 32]);
    let encoded = base64::engine::general_purpose::STANDARD.encode(key.verifying_key().as_bytes());
    Box::leak(encoded.into_boxed_str())
}

/// Every configured root decodes, or the build ships a root nothing can use.
#[test]
fn every_configured_root_decodes() {
    for entry in EMBEDDED_KEYS {
        if entry.public_key.is_empty() || entry.public_key == PLACEHOLDER_KEY {
            continue;
        }
        crate::sign::decode_public_key(entry.public_key)
            .unwrap_or_else(|error| panic!("{}: {error}", entry.key_id));
    }
}

/// The placeholder fails closed: until the coordinator pastes the real root, no key is trusted
/// and every check says `update.not_configured`.
#[test]
fn the_placeholder_root_is_never_trusted() {
    let placeholder = [EmbeddedKey {
        key_id: UPDATE_KEY_ID,
        public_key: PLACEHOLDER_KEY,
        not_after: None,
    }];
    assert!(keys_at(&placeholder, now()).is_empty());
    let store = trust_store_at(&placeholder, &[], now()).expect("store");
    assert!(store.key_ids().expect("ids").is_empty());
}

#[test]
fn the_shipped_table_names_only_the_update_key() {
    assert!(
        EMBEDDED_KEYS
            .iter()
            .all(|entry| entry.key_id.starts_with("rcommander-update-"))
    );
}

/// During the overlap both keys verify; after the old one's `not_after` only the new one.
#[test]
fn rotation_accepts_both_keys_inside_the_overlap_and_only_the_new_one_after() {
    let table = [
        EmbeddedKey {
            key_id: "rcommander-update-v1",
            public_key: public(1),
            not_after: Some("2020-01-01T00:00:00Z"),
        },
        EmbeddedKey {
            key_id: "rcommander-update-v2",
            public_key: public(2),
            not_after: None,
        },
    ];
    assert_eq!(keys_at(&table, instant("2019-06-01T00:00:00Z")).len(), 2);
    let after = keys_at(&table, instant("2021-01-01T00:00:00Z"));
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].key_id, "rcommander-update-v2");
    // A document signed by the old key no longer verifies after the window.
    let old = SigningKey::from_bytes(&[1; 32]);
    let document = sign_document("d", "rcommander-update-v1", &old, &1).expect("sign");
    let store = trust_store_at(&table, &[], instant("2021-01-01T00:00:00Z")).expect("store");
    let result: Result<u32, _> = document.verify("d", &store);
    assert!(matches!(result, Err(VerifyError::UntrustedKey { .. })));
    let store = trust_store_at(&table, &[], instant("2019-06-01T00:00:00Z")).expect("store");
    assert_eq!(document.verify::<u32>("d", &store).expect("overlap"), 1);
}

#[test]
fn an_unparseable_expiry_refuses_the_key() {
    let broken = [EmbeddedKey {
        key_id: "rcommander-update-v1",
        public_key: public(1),
        not_after: Some("whenever"),
    }];
    assert!(keys_at(&broken, now()).is_empty());
}

/// A compiled-in withdrawal refuses its document while the key vouches for everything else.
#[test]
fn a_revoked_document_is_refused_and_nothing_else() {
    let signing = SigningKey::from_bytes(&[5; 32]);
    let keys = [EmbeddedKey {
        key_id: "rcommander-update-v1",
        public_key: public(5),
        not_after: None,
    }];
    let sign = |version: &str| {
        sign_document(
            "rcommander.update-manifest.v1",
            "rcommander-update-v1",
            &signing,
            &serde_json::json!({ "version": version }),
        )
        .expect("sign")
    };
    let withdrawn = sign("1");
    let current = sign("2");
    let digest = crate::sign::hex(&withdrawn.digest("rcommander.update-manifest.v1"));
    let digest: &'static str = Box::leak(digest.into_boxed_str());
    let revoked = [RevokedDocument { digest }];
    let store = trust_store_at(&keys, &revoked, now()).expect("store");
    let refused: Result<serde_json::Value, _> =
        withdrawn.verify("rcommander.update-manifest.v1", &store);
    assert!(matches!(refused, Err(VerifyError::Revoked)));
    let accepted: Result<serde_json::Value, _> =
        current.verify("rcommander.update-manifest.v1", &store);
    assert!(accepted.is_ok());
}

#[test]
fn a_malformed_revocation_fails_the_store() {
    let revoked = [RevokedDocument { digest: "not hex" }];
    assert!(trust_store_at(EMBEDDED_KEYS, &revoked, now()).is_err());
    for entry in REVOKED_DOCUMENTS {
        decode_digest(entry.digest).unwrap_or_else(|error| panic!("{}: {error}", entry.digest));
    }
}
