use crate::{
    Db,
    connections::{
        AuthMethod, ConnectionDraft, ConnectionError, ConnectionId, ConnectionKind,
        ConnectionPatch, DefaultView, FolderId, FtpDataMode, FtpEncoding,
    },
};

fn draft(name: &str) -> ConnectionDraft {
    ConnectionDraft::new(name, "example.org")
}

#[tokio::test]
async fn a_created_connection_reads_back_with_its_defaults() {
    let db = Db::open_in_memory().await.expect("open");
    let created = db
        .create_connection(None, &draft(" web "))
        .await
        .expect("create");
    assert_eq!(created.name, "web");
    assert_eq!(created.kind, ConnectionKind::Ssh);
    assert_eq!(created.port, 22);
    assert_eq!(created.auth_method, AuthMethod::Password);
    assert_eq!(created.default_view, DefaultView::Files);
    assert_eq!(created.folder_id, None);
    assert_eq!(created.login_script_id, None);
    assert!(created.created_at.ends_with('Z'), "{}", created.created_at);
    assert_eq!(db.connection(created.id).await.expect("get"), created);
}

#[tokio::test]
async fn every_field_is_stored() {
    let db = Db::open_in_memory().await.expect("open");
    let folder = db.create_folder(None, "prod").await.expect("folder");
    let full = ConnectionDraft {
        port: Some(2222),
        username: Some("deploy".into()),
        auth_method: AuthMethod::Key,
        key_path: Some("C:\\Users\\me\\.ssh\\id_ed25519".into()),
        default_view: DefaultView::Terminal,
        start_directory: Some("/srv/www".into()),
        notes: Some("rack 4".into()),
        ..draft("web")
    };
    let created = db
        .create_connection(Some(folder.id), &full)
        .await
        .expect("create");
    assert_eq!(created.folder_id, Some(folder.id));
    assert_eq!(created.port, 2222);
    assert_eq!(created.username.as_deref(), Some("deploy"));
    assert_eq!(created.auth_method, AuthMethod::Key);
    assert_eq!(
        created.key_path.as_deref(),
        Some("C:\\Users\\me\\.ssh\\id_ed25519")
    );
    assert_eq!(created.default_view, DefaultView::Terminal);
    assert_eq!(created.start_directory.as_deref(), Some("/srv/www"));
    assert_eq!(created.notes.as_deref(), Some("rack 4"));
}

#[tokio::test]
async fn invalid_input_is_refused_with_its_code_and_writes_nothing() {
    let db = Db::open_in_memory().await.expect("open");
    let empty_host = ConnectionDraft::new("web", "  ");
    let error = db
        .create_connection(None, &empty_host)
        .await
        .expect_err("empty host");
    assert_eq!(error.code(), "host_empty");
    let port_zero = ConnectionDraft {
        port: Some(0),
        ..draft("web")
    };
    let error = db
        .create_connection(None, &port_zero)
        .await
        .expect_err("port 0");
    assert_eq!(error.code(), "port_invalid");
    let error = db
        .create_connection(Some(FolderId(99)), &draft("web"))
        .await
        .expect_err("no folder");
    assert_eq!(error.code(), "folder_not_found");
    assert!(
        db.connection_tree()
            .await
            .expect("tree")
            .connections
            .is_empty()
    );
}

#[tokio::test]
async fn a_name_is_taken_once_per_folder_ignoring_case() {
    let db = Db::open_in_memory().await.expect("open");
    let folder = db.create_folder(None, "prod").await.expect("folder");
    db.create_connection(None, &draft("Web"))
        .await
        .expect("root");
    let error = db
        .create_connection(None, &draft("web"))
        .await
        .expect_err("duplicate");
    assert!(
        matches!(&error, ConnectionError::NameTaken { name } if name == "web"),
        "{error}"
    );
    assert_eq!(error.code(), "name_taken");
    db.create_connection(Some(folder.id), &draft("web"))
        .await
        .expect("the same name in another folder");
}

#[tokio::test]
async fn an_update_changes_only_what_it_names() {
    let db = Db::open_in_memory().await.expect("open");
    let created = db
        .create_connection(
            None,
            &ConnectionDraft {
                username: Some("root".into()),
                ..draft("web")
            },
        )
        .await
        .expect("create");
    let updated = db
        .update_connection(
            created.id,
            &ConnectionPatch {
                name: Some("www".into()),
                port: Some(2200),
                username: Some(String::new()),
                ..ConnectionPatch::default()
            },
        )
        .await
        .expect("update");
    assert_eq!(updated.name, "www");
    assert_eq!(updated.port, 2200);
    assert_eq!(updated.username, None);
    assert_eq!(updated.host, "example.org");
    assert_eq!(updated.created_at, created.created_at);
}

#[tokio::test]
async fn an_update_validates_and_checks_the_name() {
    let db = Db::open_in_memory().await.expect("open");
    db.create_connection(None, &draft("db")).await.expect("db");
    let web = db
        .create_connection(None, &draft("web"))
        .await
        .expect("web");
    let taken = ConnectionPatch {
        name: Some("DB".into()),
        ..ConnectionPatch::default()
    };
    let error = db
        .update_connection(web.id, &taken)
        .await
        .expect_err("taken");
    assert_eq!(error.code(), "name_taken");
    let same = ConnectionPatch {
        name: Some("WEB".into()),
        ..ConnectionPatch::default()
    };
    let renamed = db
        .update_connection(web.id, &same)
        .await
        .expect("its own name in another case");
    assert_eq!(renamed.name, "WEB");
    let error = db
        .update_connection(ConnectionId(99), &ConnectionPatch::default())
        .await
        .expect_err("missing");
    assert_eq!(error.code(), "connection_not_found");
}

#[tokio::test]
async fn a_move_changes_the_folder_and_appends() {
    let db = Db::open_in_memory().await.expect("open");
    let folder = db.create_folder(None, "prod").await.expect("folder");
    db.create_connection(Some(folder.id), &draft("first"))
        .await
        .expect("first");
    let web = db
        .create_connection(None, &draft("web"))
        .await
        .expect("web");
    let moved = db
        .move_connection(web.id, Some(folder.id), None)
        .await
        .expect("move");
    assert_eq!(moved.folder_id, Some(folder.id));
    assert_eq!(moved.sort_order, 1);
    let placed = db
        .move_connection(web.id, None, Some(-5))
        .await
        .expect("back to the root, first");
    assert_eq!((placed.folder_id, placed.sort_order), (None, -5));
}

#[tokio::test]
async fn a_move_onto_a_taken_name_is_refused() {
    let db = Db::open_in_memory().await.expect("open");
    let folder = db.create_folder(None, "prod").await.expect("folder");
    db.create_connection(Some(folder.id), &draft("web"))
        .await
        .expect("inside");
    let web = db
        .create_connection(None, &draft("web"))
        .await
        .expect("root");
    let error = db
        .move_connection(web.id, Some(folder.id), None)
        .await
        .expect_err("taken");
    assert_eq!(error.code(), "name_taken");
}

#[tokio::test]
async fn a_delete_removes_once() {
    let db = Db::open_in_memory().await.expect("open");
    let web = db
        .create_connection(None, &draft("web"))
        .await
        .expect("web");
    db.delete_connection(web.id).await.expect("delete");
    let error = db.delete_connection(web.id).await.expect_err("again");
    assert_eq!(error.code(), "connection_not_found");
    let error = db.connection(web.id).await.expect_err("gone");
    assert_eq!(error.code(), "connection_not_found");
}

#[tokio::test]
async fn a_deleted_id_is_never_given_again() {
    let db = Db::open_in_memory().await.expect("open");
    let first = db.create_connection(None, &draft("a")).await.expect("a");
    db.delete_connection(first.id).await.expect("delete");
    let second = db
        .create_connection(None, &draft("a"))
        .await
        .expect("again");
    assert!(second.id > first.id, "{:?} after {:?}", second.id, first.id);
}

#[tokio::test]
async fn an_unknown_stored_kind_is_a_database_error_not_a_panic() {
    let db = Db::open_in_memory().await.expect("open");
    let web = db
        .create_connection(None, &draft("web"))
        .await
        .expect("web");
    {
        let mut writer = db.writer().await;
        sqlx::query("UPDATE connections SET kind = 'telnet' WHERE id = ?")
            .bind(web.id.0)
            .execute(&mut *writer)
            .await
            .expect("plant");
    }
    let error = db.connection(web.id).await.expect_err("unknown kind");
    assert_eq!(error.code(), "database_error");
}

#[tokio::test]
async fn an_ftp_connection_reads_back_with_its_kind_and_auth() {
    let db = Db::open_in_memory().await.expect("open");
    for (name, kind, auth_method) in [
        ("plain", ConnectionKind::Ftp, AuthMethod::Anonymous),
        ("explicit", ConnectionKind::Ftps, AuthMethod::Password),
        (
            "implicit",
            ConnectionKind::FtpsImplicit,
            AuthMethod::Password,
        ),
    ] {
        let created = db
            .create_connection(
                None,
                &ConnectionDraft {
                    kind,
                    auth_method,
                    username: Some("web".into()),
                    ..draft(name)
                },
            )
            .await
            .expect("create");
        assert_eq!(created.kind, kind);
        assert_eq!(created.auth_method, auth_method);
        assert_eq!(created.port, kind.default_port());
        assert_eq!(db.connection(created.id).await.expect("get"), created);
    }
}

#[tokio::test]
async fn connections_are_found_by_name_in_every_folder() {
    let db = Db::open_in_memory().await.expect("open");
    let folder = db.create_folder(None, "nas").await.expect("folder");
    let root = db
        .create_connection(None, &draft("Box"))
        .await
        .expect("root");
    let inside = db
        .create_connection(Some(folder.id), &draft("box"))
        .await
        .expect("inside");
    db.create_connection(None, &draft("other"))
        .await
        .expect("other");
    let found: Vec<ConnectionId> = db
        .connections_named(" BOX ")
        .await
        .expect("named")
        .iter()
        .map(|connection| connection.id)
        .collect();
    assert_eq!(found, vec![root.id, inside.id]);
    assert!(db.connections_named("none").await.expect("none").is_empty());
}

#[tokio::test]
async fn the_ftp_options_are_stored_and_changed() {
    let db = Db::open_in_memory().await.expect("open");
    let created = db
        .create_connection(
            None,
            &ConnectionDraft {
                kind: ConnectionKind::Ftps,
                auth_method: AuthMethod::Anonymous,
                ftp_data_mode: Some(FtpDataMode::Active),
                ..draft("nas")
            },
        )
        .await
        .expect("create");
    assert_eq!(created.ftp_data_mode, Some(FtpDataMode::Active));
    assert_eq!(created.ftp_encoding, None, "unset is the default");
    let changed = db
        .update_connection(
            created.id,
            &ConnectionPatch {
                ftp_encoding: Some(FtpEncoding::Utf8),
                ..ConnectionPatch::default()
            },
        )
        .await
        .expect("update");
    assert_eq!(changed.ftp_data_mode, Some(FtpDataMode::Active));
    assert_eq!(changed.ftp_encoding, Some(FtpEncoding::Utf8));
    assert_eq!(db.connection(created.id).await.expect("get"), changed);
}
