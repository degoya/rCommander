use crate::{
    Db,
    connections::{
        ConnectionDraft, ConnectionError, ConnectionKind, FtpDataMode, FtpEncoding,
        INSTALLATION_KEY, check_draft,
    },
};

fn draft(name: &str) -> ConnectionDraft {
    ConnectionDraft::new(name, "example.org")
}

#[tokio::test]
async fn a_marked_connection_keeps_its_mark_and_every_field() {
    let db = Db::open_in_memory().await.expect("open");
    let folder = db.create_folder(None, "nas").await.expect("folder");
    let ftp = ConnectionDraft {
        kind: ConnectionKind::Ftps,
        ftp_data_mode: Some(FtpDataMode::Active),
        ftp_encoding: Some(FtpEncoding::Utf8),
        username: Some("alice".into()),
        ..draft("files")
    };
    let made = db
        .create_marked_connection(Some(folder.id), &ftp, "rcexport:abc", "7")
        .await
        .expect("created");
    assert_eq!(made.folder_id, Some(folder.id));
    assert_eq!(made.ftp_data_mode, Some(FtpDataMode::Active));
    assert_eq!(made.ftp_encoding, Some(FtpEncoding::Utf8));
    let plain = db
        .create_connection(None, &draft("plain"))
        .await
        .expect("created");
    let origins = db.connection_origins().await.expect("origins");
    assert_eq!(origins.len(), 2);
    assert_eq!(origins[0].id, made.id);
    assert_eq!(origins[0].source.as_deref(), Some("rcexport:abc"));
    assert_eq!(origins[0].key.as_deref(), Some("7"));
    assert_eq!(origins[1].id, plain.id);
    assert_eq!(origins[1].source, None);
}

#[tokio::test]
async fn a_mark_is_taken_once() {
    let db = Db::open_in_memory().await.expect("open");
    db.create_marked_connection(None, &draft("one"), "rcexport:abc", "7")
        .await
        .expect("created");
    let second = db
        .create_marked_connection(None, &draft("two"), "rcexport:abc", "7")
        .await;
    assert!(matches!(second, Err(ConnectionError::NameTaken { .. })));
    let taken_name = db
        .create_marked_connection(None, &draft("ONE"), "rcexport:abc", "8")
        .await;
    assert!(matches!(taken_name, Err(ConnectionError::NameTaken { .. })));
}

#[tokio::test]
async fn the_installation_id_is_drawn_once() {
    let db = Db::open_in_memory().await.expect("open");
    let first = db.installation_id().await.expect("id");
    assert_eq!(first.len(), 32);
    assert!(
        first
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
    assert_eq!(db.installation_id().await.expect("id"), first);
    let stored: Option<String> = db.get(INSTALLATION_KEY).await.expect("read");
    assert_eq!(stored.as_deref(), Some(first.as_str()));
}

#[test]
fn a_draft_is_checked_without_a_database() {
    assert!(check_draft(&draft("web")).is_ok());
    let blank = check_draft(&ConnectionDraft::new("  ", "example.org"));
    assert!(matches!(blank, Err(ConnectionError::NameEmpty)));
    let port = check_draft(&ConnectionDraft {
        port: Some(70_000),
        ..draft("web")
    });
    assert!(port.is_err());
}
