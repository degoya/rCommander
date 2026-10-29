use crate::{
    Db, SecretKind, StoredKey,
    connections::{
        Connection, ConnectionDraft, ConnectionError, ConnectionId, ConnectionPatch,
        ConnectionSave, SaveError, SaveTarget, SecretWrite,
    },
    login_scripts::{LoginScriptError, LoginScriptFields, LoginScriptId},
};

/// What a test's `setting` edit writes: the ids it was given, after the saved one.
type Marks = Vec<i64>;

/// The edit of a save that rewrites no setting.
type NoEdit = fn(Option<Marks>, &Connection, &[ConnectionId]) -> Marks;

const MARKS: &str = "test.marks";

/// A "ciphertext" that names what it was sealed for.
fn sealed(kind: SecretKind, row: &str) -> Result<Vec<u8>, &'static str> {
    Ok(format!("{}/{row}", kind.as_str()).into_bytes())
}

fn key() -> StoredKey {
    StoredKey {
        key_type: "ssh-ed25519".into(),
        fingerprint: "SHA256:abc".into(),
        comment: Some("me@box".into()),
        stored: true,
        public_key: "ssh-ed25519 AAAA".into(),
    }
}

async fn script(db: &Db) -> LoginScriptId {
    db.create_login_script(&LoginScriptFields {
        name: "root",
        schema_version: 1,
        steps: r#"[{"step":"send_text","text":"sudo -i","newline":true}]"#,
        step_timeout_ms: 15_000,
        total_timeout_ms: 120_000,
    })
    .await
    .expect("script")
    .id
}

#[tokio::test]
async fn a_new_connection_is_saved_with_its_secrets_script_and_setting_at_once() {
    let db = Db::open_in_memory().await.expect("open");
    let script = script(&db).await;
    let other = db
        .create_connection(None, &ConnectionDraft::new("other", "b.example.org"))
        .await
        .expect("other");
    let draft = ConnectionDraft::new("web", "example.org");
    let info = key();
    let save = ConnectionSave {
        target: SaveTarget::Create {
            folder: None,
            draft: &draft,
        },
        secrets: vec![
            SecretWrite::Seal(SecretKind::Password),
            SecretWrite::Key {
                info: &info,
                passphrase: true,
            },
        ],
        login_script: Some(Some(script)),
        credential: None,
    };
    let edit = |stored: Option<Marks>, saved: &Connection, ids: &[ConnectionId]| {
        assert_eq!(stored, None, "nothing stored yet");
        let mut marks: Marks = ids.iter().map(|id| id.0).collect();
        marks.retain(|id| *id != saved.id.0);
        marks.push(saved.id.0);
        marks
    };
    let saved = db
        .save_connection(&save, sealed, Some((MARKS, edit)))
        .await
        .expect("saved");
    assert_eq!(saved.name, "web");
    assert_eq!(saved.login_script_id, Some(script.0));
    assert!(saved.secrets.has_password && saved.secrets.has_key && saved.secrets.has_passphrase);
    assert_eq!(saved.key, Some(info));
    // Each secret was sealed for the id the connection got inside the transaction.
    let row = saved.id.0.to_string();
    for kind in SecretKind::ALL {
        assert_eq!(
            db.connection_secret(saved.id, kind).await.expect("read"),
            Some(format!("{}/{row}", kind.as_str()).into_bytes())
        );
    }
    let marks: Option<Marks> = db.get(MARKS).await.expect("setting");
    assert_eq!(marks, Some(vec![other.id.0, saved.id.0]));
}

#[tokio::test]
async fn a_failing_seal_leaves_nothing_written() {
    let db = Db::open_in_memory().await.expect("open");
    let draft = ConnectionDraft::new("web", "example.org");
    let save = ConnectionSave {
        target: SaveTarget::Create {
            folder: None,
            draft: &draft,
        },
        secrets: vec![SecretWrite::Seal(SecretKind::Password)],
        login_script: None,
        credential: None,
    };
    let refused = db
        .save_connection(&save, |_, _| Err("locked"), None::<(&str, NoEdit)>)
        .await
        .expect_err("refused");
    assert!(matches!(refused, SaveError::Seal("locked")), "{refused:?}");
    assert!(
        db.connection_tree()
            .await
            .expect("tree")
            .connections
            .is_empty(),
        "the new connection went with the transaction"
    );

    // A stored one keeps its fields and its secrets.
    let stored = db.create_connection(None, &draft).await.expect("stored");
    db.set_connection_secret(stored.id, SecretKind::Passphrase, Some(b"old"))
        .await
        .expect("secret");
    let before = db.connection(stored.id).await.expect("before");
    let patch = ConnectionPatch {
        notes: Some("changed".into()),
        ..ConnectionPatch::default()
    };
    let save = ConnectionSave {
        target: SaveTarget::Update {
            id: stored.id,
            patch: &patch,
        },
        secrets: vec![
            SecretWrite::Clear(SecretKind::Passphrase),
            SecretWrite::Seal(SecretKind::Password),
        ],
        login_script: None,
        credential: None,
    };
    db.save_connection(&save, |_, _| Err("locked"), None::<(&str, NoEdit)>)
        .await
        .expect_err("refused");
    assert_eq!(db.connection(stored.id).await.expect("after"), before);
    assert_eq!(
        db.connection_secret(stored.id, SecretKind::Passphrase)
            .await
            .expect("read"),
        Some(b"old".to_vec())
    );
}

#[tokio::test]
async fn a_missing_script_or_a_taken_name_leaves_the_connection_as_it_was() {
    let db = Db::open_in_memory().await.expect("open");
    let stored = db
        .create_connection(None, &ConnectionDraft::new("web", "example.org"))
        .await
        .expect("stored");
    db.create_connection(None, &ConnectionDraft::new("db", "example.org"))
        .await
        .expect("second");
    let before = db.connection(stored.id).await.expect("before");
    let patch = ConnectionPatch {
        host: Some("new.example.org".into()),
        ..ConnectionPatch::default()
    };
    let save = ConnectionSave {
        target: SaveTarget::Update {
            id: stored.id,
            patch: &patch,
        },
        secrets: vec![SecretWrite::Seal(SecretKind::Password)],
        login_script: Some(Some(LoginScriptId(99))),
        credential: None,
    };
    let refused = db
        .save_connection(&save, sealed, None::<(&str, NoEdit)>)
        .await
        .expect_err("refused");
    assert!(
        matches!(
            refused,
            SaveError::LoginScript(LoginScriptError::NotFound { .. })
        ),
        "{refused:?}"
    );
    assert_eq!(db.connection(stored.id).await.expect("after"), before);

    let taken = ConnectionPatch {
        name: Some("DB".into()),
        ..ConnectionPatch::default()
    };
    let target = SaveTarget::Update {
        id: stored.id,
        patch: &taken,
    };
    let checked = db.check_connection_save(target).await;
    assert!(
        matches!(checked, Err(ConnectionError::NameTaken { .. })),
        "{checked:?}"
    );
    let save = ConnectionSave {
        target,
        secrets: Vec::new(),
        login_script: None,
        credential: None,
    };
    let refused = db
        .save_connection(&save, sealed, None::<(&str, NoEdit)>)
        .await
        .expect_err("refused");
    assert!(
        matches!(
            refused,
            SaveError::Connection(ConnectionError::NameTaken { .. })
        ),
        "{refused:?}"
    );
    assert_eq!(db.connection(stored.id).await.expect("after"), before);
}

#[tokio::test]
async fn an_empty_patch_writes_no_field() {
    let db = Db::open_in_memory().await.expect("open");
    let stored = db
        .create_connection(None, &ConnectionDraft::new("web", "example.org"))
        .await
        .expect("stored");
    let patch = ConnectionPatch::default();
    let target = SaveTarget::Update {
        id: stored.id,
        patch: &patch,
    };
    db.check_connection_save(target)
        .await
        .expect("nothing to refuse");
    let save = ConnectionSave {
        target,
        secrets: vec![SecretWrite::Clear(SecretKind::Password)],
        login_script: None,
        credential: None,
    };
    let saved = db
        .save_connection(&save, sealed, None::<(&str, NoEdit)>)
        .await
        .expect("saved");
    assert_eq!(saved.name, stored.name);
    let missing = SaveTarget::Update {
        id: ConnectionId(99),
        patch: &patch,
    };
    assert!(matches!(
        db.check_connection_save(missing).await,
        Err(ConnectionError::ConnectionNotFound { .. })
    ));
}
