use super::*;
use crate::connections::ConnectionDraft;

const SOURCE: &str = "plugin:019d0000-0000-7000-8000-0000000000b1";

async fn with_connection() -> (Db, ConnectionId) {
    let db = Db::open_in_memory().await.expect("open");
    let draft = ConnectionDraft::new("web", "example.org");
    let id = db.create_connection(None, &draft).await.expect("create").id;
    (db, id)
}

fn reference(id: ConnectionId, kind: SecretKind, entry: &str) -> SecretSourceRef {
    SecretSourceRef {
        connection_id: id,
        kind,
        source: SOURCE.to_owned(),
        entry_id: entry.to_owned(),
        entry_label: format!("{entry} (root)"),
    }
}

#[tokio::test]
async fn a_reference_is_set_replaced_read_and_removed() {
    let (db, id) = with_connection().await;
    assert_eq!(db.secret_sources(id).await.expect("read"), Vec::new());
    db.set_secret_source(&reference(id, SecretKind::Password, "a"))
        .await
        .expect("set");
    db.set_secret_source(&reference(id, SecretKind::Password, "b"))
        .await
        .expect("replace");
    db.set_secret_source(&reference(id, SecretKind::Key, "k"))
        .await
        .expect("set");
    assert_eq!(
        db.secret_source(id, SecretKind::Password)
            .await
            .expect("read"),
        Some(reference(id, SecretKind::Password, "b"))
    );
    assert_eq!(db.secret_sources(id).await.expect("read").len(), 2);
    // The vault's columns are untouched: a reference is no stored secret.
    assert_eq!(
        db.connection(id).await.expect("read").secrets,
        crate::SecretFlags::default()
    );

    assert!(
        db.remove_secret_source(id, SecretKind::Password)
            .await
            .expect("remove")
    );
    assert!(
        !db.remove_secret_source(id, SecretKind::Password)
            .await
            .expect("again")
    );
    assert_eq!(
        db.secret_source(id, SecretKind::Password)
            .await
            .expect("read"),
        None
    );
}

#[tokio::test]
async fn a_missing_connection_or_a_bad_source_is_refused() {
    let (db, id) = with_connection().await;
    let gone = ConnectionId(id.0 + 1);
    assert!(matches!(
        db.set_secret_source(&reference(gone, SecretKind::Password, "a"))
            .await,
        Err(ConnectionError::ConnectionNotFound { .. })
    ));
    for source in ["plugin:short", "keepass", ""] {
        let bad = SecretSourceRef {
            source: source.to_owned(),
            ..reference(id, SecretKind::Password, "a")
        };
        assert!(db.set_secret_source(&bad).await.is_err(), "{source}");
    }
    let wincred = SecretSourceRef {
        source: "wincred".to_owned(),
        ..reference(id, SecretKind::Password, "a")
    };
    db.set_secret_source(&wincred).await.expect("wincred");
}

#[tokio::test]
async fn deleting_a_connection_deletes_its_references() {
    let (db, id) = with_connection().await;
    db.set_secret_source(&reference(id, SecretKind::Password, "a"))
        .await
        .expect("set");
    db.delete_connection(id).await.expect("delete");
    assert_eq!(db.secret_sources(id).await.expect("read"), Vec::new());
}
