use super::*;
use crate::{
    SECRET_COLUMNS,
    connections::{ConnectionDraft, ConnectionSave, SaveTarget},
};

/// A "ciphertext" that names what it was sealed for.
fn sealed(kind: SecretKind, row: &str) -> Result<Vec<u8>, &'static str> {
    Ok(format!("{}/{row}", kind.as_str()).into_bytes())
}

fn draft(name: &str, user: Option<&str>) -> CredentialDraft {
    CredentialDraft {
        name: name.into(),
        username: user.map(str::to_owned),
    }
}

async fn create(db: &Db, name: &str, secrets: Vec<SecretWrite<'_>>) -> Credential {
    let draft = draft(name, Some("deploy"));
    let save = CredentialSave {
        target: CredentialTarget::Create {
            folder: None,
            draft: &draft,
        },
        secrets,
    };
    db.save_credential(&save, sealed).await.expect("saved")
}

/// Points connection `id` at `credential` as the manager's form does.
async fn link(db: &Db, id: ConnectionId, credential: Option<CredentialId>) {
    let patch = crate::connections::ConnectionPatch::default();
    let save = ConnectionSave {
        target: SaveTarget::Update { id, patch: &patch },
        secrets: Vec::new(),
        login_script: None,
        credential: Some(credential),
    };
    type NoEdit = fn(Option<i64>, &crate::connections::Connection, &[ConnectionId]) -> i64;
    db.save_connection(&save, sealed, None::<(&str, NoEdit)>)
        .await
        .expect("linked");
}

async fn connection(db: &Db, name: &str) -> ConnectionId {
    db.create_connection(None, &ConnectionDraft::new(name, "example.org"))
        .await
        .expect("connection")
        .id
}

#[tokio::test]
async fn an_entry_is_saved_with_its_secrets_sealed_for_its_own_row() {
    let db = Db::open_in_memory().await.expect("open");
    let saved = create(&db, "ops", vec![SecretWrite::Seal(SecretKind::Password)]).await;
    assert_eq!(saved.name, "ops");
    assert_eq!(saved.username.as_deref(), Some("deploy"));
    assert!(saved.secrets.has_password && !saved.secrets.has_key);
    let owner = SecretOwner::Credential(saved.id);
    let stored = db.stored_secret(owner, SecretKind::Password).await;
    assert_eq!(
        stored.expect("read"),
        Some(format!("password/{}", saved.id).into_bytes())
    );
    assert_eq!(db.credentials().await.expect("list"), [saved]);
}

#[tokio::test]
async fn two_connections_on_one_entry_are_counted_and_read_it() {
    let db = Db::open_in_memory().await.expect("open");
    let shared = create(&db, "ops", vec![SecretWrite::Seal(SecretKind::Password)]).await;
    let (web, db_host) = (connection(&db, "web").await, connection(&db, "db").await);
    link(&db, web, Some(shared.id)).await;
    link(&db, db_host, Some(shared.id)).await;
    assert_eq!(db.credential(shared.id).await.expect("read").connections, 2);
    for id in [web, db_host] {
        assert_eq!(
            db.connection(id).await.expect("read").credential_id,
            Some(shared.id)
        );
        let of = db.credential_of(id).await.expect("read").expect("linked");
        assert_eq!(of.id, shared.id);
    }
    link(&db, web, None).await;
    assert_eq!(db.credential_of(web).await.expect("read"), None);
}

#[tokio::test]
async fn a_link_to_a_missing_entry_is_refused_and_nothing_written() {
    let db = Db::open_in_memory().await.expect("open");
    let id = connection(&db, "web").await;
    let patch = crate::connections::ConnectionPatch {
        name: Some("renamed".into()),
        ..Default::default()
    };
    let save = ConnectionSave {
        target: SaveTarget::Update { id, patch: &patch },
        secrets: Vec::new(),
        login_script: None,
        credential: Some(Some(CredentialId(99))),
    };
    type NoEdit = fn(Option<i64>, &crate::connections::Connection, &[ConnectionId]) -> i64;
    let result = db
        .save_connection(&save, sealed, None::<(&str, NoEdit)>)
        .await;
    assert!(matches!(
        result,
        Err(SaveError::Connection(
            ConnectionError::CredentialNotFound { .. }
        ))
    ));
    assert_eq!(db.connection(id).await.expect("read").name, "web");
}

#[tokio::test]
async fn deleting_an_entry_answers_its_users_and_they_fall_back() {
    let db = Db::open_in_memory().await.expect("open");
    let shared = create(&db, "ops", vec![SecretWrite::Seal(SecretKind::Password)]).await;
    let web = connection(&db, "web").await;
    link(&db, web, Some(shared.id)).await;
    assert_eq!(db.stored_secrets(SECRET_COLUMNS).await.expect("count"), 1);
    assert_eq!(db.delete_credential(shared.id).await.expect("deleted"), 1);
    assert_eq!(db.connection(web).await.expect("read").credential_id, None);
    assert_eq!(db.stored_secrets(SECRET_COLUMNS).await.expect("count"), 0);
    assert!(matches!(
        db.delete_credential(shared.id).await,
        Err(ConnectionError::CredentialNotFound { .. })
    ));
}

#[tokio::test]
async fn a_patch_renames_clears_the_user_and_keeps_the_secrets() {
    let db = Db::open_in_memory().await.expect("open");
    let shared = create(&db, "ops", vec![SecretWrite::Seal(SecretKind::Password)]).await;
    let patch = CredentialPatch {
        name: Some(" admins ".into()),
        username: Some(String::new()),
    };
    let save = CredentialSave {
        target: CredentialTarget::Update {
            id: shared.id,
            patch: &patch,
        },
        secrets: vec![SecretWrite::Clear(SecretKind::Password)],
    };
    let saved = db.save_credential(&save, sealed).await.expect("saved");
    assert_eq!(saved.name, "admins");
    assert_eq!(saved.username, None);
    assert!(!saved.secrets.has_password);
}

#[tokio::test]
async fn names_are_once_per_folder_and_checked_before_writing() {
    let db = Db::open_in_memory().await.expect("open");
    create(&db, "ops", Vec::new()).await;
    let taken = draft("OPS", None);
    let target = CredentialTarget::Create {
        folder: None,
        draft: &taken,
    };
    assert!(matches!(
        db.check_credential_save(target).await,
        Err(ConnectionError::NameTaken { .. })
    ));
    let folder = db.create_folder(None, "lab").await.expect("folder").id;
    let in_folder = CredentialTarget::Create {
        folder: Some(folder),
        draft: &taken,
    };
    db.check_credential_save(in_folder)
        .await
        .expect("free in another folder");
    let blank = draft(" ", None);
    let target = CredentialTarget::Create {
        folder: None,
        draft: &blank,
    };
    assert!(matches!(
        db.check_credential_save(target).await,
        Err(ConnectionError::NameEmpty)
    ));
}

#[tokio::test]
async fn an_entry_moves_between_folders_and_goes_with_its_folder() {
    let db = Db::open_in_memory().await.expect("open");
    let shared = create(&db, "ops", Vec::new()).await;
    let folder = db.create_folder(None, "lab").await.expect("folder").id;
    let moved = db
        .move_credential(shared.id, Some(folder))
        .await
        .expect("moved");
    assert_eq!(moved.folder_id, Some(folder));
    // A folder holding an entry is not empty.
    assert!(matches!(
        db.delete_folder(folder, false).await,
        Err(ConnectionError::FolderNotEmpty { .. })
    ));
    db.delete_folder(folder, true).await.expect("deleted");
    assert_eq!(db.credentials().await.expect("list"), []);
}

#[tokio::test]
async fn a_new_vault_key_is_checked_before_a_secret_is_stored() {
    let db = Db::open_in_memory().await.expect("open");
    let shared = create(&db, "ops", Vec::new()).await;
    db.hold_vault_key(Some(vec![1; 16]));
    let save = CredentialSave {
        target: CredentialTarget::Update {
            id: shared.id,
            patch: &CredentialPatch::default(),
        },
        secrets: vec![SecretWrite::Seal(SecretKind::Password)],
    };
    let result = db.save_credential(&save, sealed).await;
    assert!(matches!(
        result,
        Err(SaveError::Connection(ConnectionError::Db(
            crate::DbError::VaultChanged
        )))
    ));
}

#[tokio::test]
async fn the_tree_shows_each_entry_in_its_folder() {
    let db = Db::open_in_memory().await.expect("open");
    let root = create(&db, "ops", Vec::new()).await;
    let lab = create(&db, "lab admin", Vec::new()).await;
    let folder = db.create_folder(None, "lab").await.expect("folder").id;
    db.move_credential(lab.id, Some(folder))
        .await
        .expect("moved");
    let tree = db.connection_tree().await.expect("tree");
    let names = |credentials: &[Credential]| -> Vec<String> {
        credentials.iter().map(|c| c.name.clone()).collect()
    };
    assert_eq!(names(&tree.credentials), [root.name]);
    assert_eq!(names(&tree.folders[0].credentials), [lab.name]);
}
