use super::*;

const KEY: &str = "B0zGe1KstAvHnny52BYAl7rrW3l5SVnBy8Fmf8NVkt0=";

static ROTATING: [EmbeddedKey; 3] = [
    EmbeddedKey {
        key_id: "old",
        public_key: KEY,
        not_after: Some(1_000),
    },
    EmbeddedKey {
        key_id: "new",
        public_key: KEY,
        not_after: None,
    },
    EmbeddedKey {
        key_id: "unset",
        public_key: "",
        not_after: None,
    },
];

#[test]
fn every_configured_root_decodes_and_the_store_trusts_it() {
    for entry in EMBEDDED_KEYS {
        crate::sign::decode_public_key(entry.public_key)
            .unwrap_or_else(|error| panic!("{}: {error}", entry.key_id));
    }
    let store = TrustStore::with_roots().expect("roots");
    assert_eq!(
        store.key_ids().expect("ids"),
        [DEVELOPMENT_KEY_ID, super::RELEASE_KEY_ID]
    );
}

#[test]
fn both_keys_count_inside_the_overlap_and_only_the_new_one_after_it() {
    let ids = |now| {
        root_keys_at(&ROTATING, now)
            .iter()
            .map(|key| key.key_id)
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(500), ["old", "new"]);
    assert_eq!(ids(1_001), ["new"]);
}
