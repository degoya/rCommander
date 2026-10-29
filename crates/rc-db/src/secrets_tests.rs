use super::*;
use crate::{
    SECRET_COLUMNS,
    connections::{AuthMethod, ConnectionDraft},
};

async fn with_connection() -> (Db, ConnectionId) {
    let db = Db::open_in_memory().await.expect("open");
    let draft = ConnectionDraft {
        auth_method: AuthMethod::Key,
        ..ConnectionDraft::new("web", "example.org")
    };
    let id = db.create_connection(None, &draft).await.expect("create").id;
    (db, id)
}

#[tokio::test]
async fn a_new_connection_has_no_secrets() {
    let (db, id) = with_connection().await;
    let connection = db.connection(id).await.expect("read");
    assert_eq!(connection.secrets, SecretFlags::default());
    for kind in SecretKind::ALL {
        assert_eq!(db.connection_secret(id, kind).await.expect("read"), None);
    }
}

#[tokio::test]
async fn each_kind_is_stored_in_its_own_column_and_flagged() {
    let (db, id) = with_connection().await;
    for (index, kind) in SecretKind::ALL.into_iter().enumerate() {
        let value = vec![u8::try_from(index).expect("small"); 4];
        let connection = db
            .set_connection_secret(id, kind, Some(value.as_slice()))
            .await
            .expect("store");
        assert!(connection.secrets.has(kind), "{kind:?}");
        assert_eq!(
            db.connection_secret(id, kind).await.expect("read"),
            Some(value)
        );
    }
    let tree = db.connection_tree().await.expect("tree");
    assert_eq!(
        tree.connections[0].secrets,
        SecretFlags {
            has_password: true,
            has_key: true,
            has_passphrase: true,
        }
    );
    assert_eq!(db.stored_secrets(SECRET_COLUMNS).await.expect("count"), 3);

    let cleared = db
        .set_connection_secret(id, SecretKind::Key, None)
        .await
        .expect("clear");
    assert!(!cleared.secrets.has_key);
    assert!(cleared.secrets.has_password && cleared.secrets.has_passphrase);
    assert_eq!(db.stored_secrets(SECRET_COLUMNS).await.expect("count"), 2);
}

#[tokio::test]
async fn a_missing_connection_is_reported() {
    let (db, id) = with_connection().await;
    let gone = ConnectionId(id.0 + 1);
    assert!(matches!(
        db.connection_secret(gone, SecretKind::Password).await,
        Err(ConnectionError::ConnectionNotFound { .. })
    ));
    assert!(matches!(
        db.set_connection_secret(gone, SecretKind::Password, Some(b"x".as_slice()))
            .await,
        Err(ConnectionError::ConnectionNotFound { .. })
    ));
}

#[tokio::test]
async fn deleting_a_connection_deletes_its_secrets() {
    let (db, id) = with_connection().await;
    db.set_connection_secret(id, SecretKind::Password, Some(b"cipher".as_slice()))
        .await
        .expect("store");
    db.delete_connection(id).await.expect("delete");
    assert_eq!(db.stored_secrets(SECRET_COLUMNS).await.expect("count"), 0);
}

#[test]
fn the_flags_serialize_as_has_fields_only() {
    let value = serde_json::to_value(SecretFlags {
        has_password: true,
        ..SecretFlags::default()
    })
    .expect("serializes");
    assert_eq!(
        value,
        serde_json::json!({ "has_password": true, "has_key": false, "has_passphrase": false })
    );
}

#[test]
fn every_kind_names_a_secret_column() {
    for kind in SecretKind::ALL {
        assert!(
            SECRET_COLUMNS
                .iter()
                .any(|column| column.table == SECRETS_TABLE && column.column == kind.column()),
            "{kind:?}"
        );
        assert_eq!(
            serde_json::to_value(kind).expect("serializes"),
            kind.as_str()
        );
    }
}

#[tokio::test]
async fn a_stored_key_keeps_its_public_half_until_it_is_cleared() {
    let (db, id) = with_connection().await;
    let info = StoredKey {
        key_type: "ssh-ed25519".into(),
        fingerprint: "SHA256:abc".into(),
        comment: Some("me@box".into()),
        stored: true,
        public_key: "ssh-ed25519 AAAA".into(),
    };
    let connection = db
        .store_connection_key(id, b"key-cipher", &info, Some(b"pp-cipher".as_slice()))
        .await
        .expect("store");
    assert_eq!(connection.key.as_ref(), Some(&info));
    assert!(connection.secrets.has_key && connection.secrets.has_passphrase);
    let value = serde_json::to_value(&connection).expect("serializes");
    assert_eq!(
        value["key"],
        serde_json::json!({
            "type": "ssh-ed25519",
            "fingerprint": "SHA256:abc",
            "comment": "me@box",
            "stored": true,
        })
    );

    // Without a new passphrase the stored one stays.
    db.store_connection_key(id, b"key-cipher-2", &info, None)
        .await
        .expect("store again");
    assert_eq!(
        db.connection_secret(id, SecretKind::Passphrase)
            .await
            .expect("read"),
        Some(b"pp-cipher".to_vec())
    );

    let cleared = db
        .set_connection_secret(id, SecretKind::Key, None)
        .await
        .expect("clear");
    assert_eq!(cleared.key, None);
    assert!(!cleared.secrets.has_key);
}

/// RC-080-11 (V4): while the vault holds a key, a secret is stored only when `vault_meta` still
/// has that key's salt - checked in the write's own transaction; removing a secret needs no key.
#[tokio::test]
async fn a_secret_write_under_a_stale_vault_key_is_refused() {
    let (db, id) = with_connection().await;
    let mut meta = crate::VaultMeta {
        version: 1,
        kdf: "argon2id".to_owned(),
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
        salt: vec![1; 16],
        verifier: vec![0xAB; 40],
    };
    db.insert_vault_meta(&meta).await.expect("vault row");
    db.hold_vault_key(Some(vec![1; 16]));
    db.set_connection_secret(id, SecretKind::Password, Some(&[1, 2, 3][..]))
        .await
        .expect("the key is the vault's");

    // Another process changed the master password: a new salt in the row.
    meta.salt = vec![2; 16];
    db.rekey_secrets(SECRET_COLUMNS, &meta, |_, _, value| {
        Ok::<_, ()>(value.to_vec())
    })
    .await
    .expect("the other process's change");
    let refused = db
        .set_connection_secret(id, SecretKind::Password, Some(&[9, 9, 9][..]))
        .await
        .expect_err("a stale key");
    assert!(matches!(
        refused,
        ConnectionError::Db(crate::DbError::VaultChanged)
    ));
    assert_eq!(
        db.connection_secret(id, SecretKind::Password)
            .await
            .expect("read"),
        Some(vec![1, 2, 3]),
        "nothing was written"
    );
    let info = StoredKey {
        key_type: "ssh-ed25519".to_owned(),
        fingerprint: "SHA256:x".to_owned(),
        comment: None,
        stored: true,
        public_key: String::new(),
    };
    assert!(
        db.store_connection_key(id, &[7], &info, None)
            .await
            .is_err()
    );
    db.set_connection_secret(id, SecretKind::Password, None)
        .await
        .expect("removing needs no key");

    db.hold_vault_key(Some(vec![2; 16]));
    db.set_connection_secret(id, SecretKind::Password, Some(&[4][..]))
        .await
        .expect("unlocked again under the new salt");
    db.hold_vault_key(None);
    db.set_connection_secret(id, SecretKind::Password, Some(&[5][..]))
        .await
        .expect("no key held, nothing checked");
}
