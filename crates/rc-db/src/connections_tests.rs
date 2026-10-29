use sqlx::{Connection as _, SqliteConnection, sqlite::SqliteConnectOptions};

use super::*;
use crate::{DB_FILE, Db, DbError, MIGRATOR};

/// One error of every variant; the match stops compiling when one is added without a sample.
fn one_of_each() -> Vec<ConnectionError> {
    let samples = vec![
        ConnectionError::ConnectionNotFound {
            id: ConnectionId(1),
        },
        ConnectionError::FolderNotFound { id: FolderId(1) },
        ConnectionError::CredentialNotFound {
            id: CredentialId(1),
        },
        ConnectionError::NameEmpty,
        ConnectionError::NameTaken { name: "x".into() },
        ConnectionError::HostEmpty,
        ConnectionError::HostInvalid,
        ConnectionError::PortInvalid { port: 0 },
        ConnectionError::FolderCycle,
        ConnectionError::FolderTooDeep { max: MAX_DEPTH },
        ConnectionError::FolderNotEmpty { id: FolderId(1) },
        ConnectionError::AuthUnsupported {
            kind: ConnectionKind::Ftp,
            auth_method: AuthMethod::Key,
        },
        ConnectionError::UsernameEmpty,
        ConnectionError::ViewUnsupported {
            kind: ConnectionKind::Ftp,
        },
        ConnectionError::ControlCharacter { field: "name" },
        ConnectionError::TextTooLong {
            field: "name",
            max: MAX_TEXT_CHARS,
        },
        ConnectionError::PathTooLong {
            field: "key path",
            max: MAX_PATH_CHARS,
        },
        ConnectionError::EndpointChangeNeedsGui {
            id: ConnectionId(1),
        },
        ConnectionError::Db(DbError::NoDataDir),
    ];
    for sample in &samples {
        match sample {
            ConnectionError::ConnectionNotFound { .. }
            | ConnectionError::FolderNotFound { .. }
            | ConnectionError::CredentialNotFound { .. }
            | ConnectionError::NameEmpty
            | ConnectionError::NameTaken { .. }
            | ConnectionError::HostEmpty
            | ConnectionError::HostInvalid
            | ConnectionError::PortInvalid { .. }
            | ConnectionError::FolderCycle
            | ConnectionError::FolderTooDeep { .. }
            | ConnectionError::FolderNotEmpty { .. }
            | ConnectionError::AuthUnsupported { .. }
            | ConnectionError::UsernameEmpty
            | ConnectionError::ViewUnsupported { .. }
            | ConnectionError::ControlCharacter { .. }
            | ConnectionError::TextTooLong { .. }
            | ConnectionError::PathTooLong { .. }
            | ConnectionError::EndpointChangeNeedsGui { .. }
            | ConnectionError::Db(_) => {}
        }
    }
    samples
}

#[test]
fn all_codes_lists_every_code_once() {
    let codes: Vec<&str> = one_of_each().iter().map(ConnectionError::code).collect();
    assert_eq!(codes, ConnectionError::ALL_CODES);
}

#[test]
fn the_enums_read_back_what_they_store() {
    for kind in ConnectionKind::ALL {
        assert_eq!(ConnectionKind::parse(kind.as_str()), Some(*kind));
        assert_eq!(
            serde_json::to_value(kind).expect("serializes"),
            kind.as_str()
        );
    }
    for method in AuthMethod::ALL {
        assert_eq!(AuthMethod::parse(method.as_str()), Some(*method));
        assert_eq!(
            serde_json::to_value(method).expect("serializes"),
            method.as_str()
        );
    }
    for view in DefaultView::ALL {
        assert_eq!(DefaultView::parse(view.as_str()), Some(*view));
    }
    assert_eq!(AuthMethod::parse("Password"), None);
}

#[tokio::test]
async fn the_tree_survives_a_restart() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(DB_FILE);
    let db = Db::open(&path).await.expect("first open");
    let prod = db.create_folder(None, "prod").await.expect("folder");
    let web = db
        .create_connection(Some(prod.id), &ConnectionDraft::new("web", "example.org"))
        .await
        .expect("connection");
    let before = db.connection_tree().await.expect("tree");
    db.close().await;

    let db = Db::open(&path).await.expect("second open");
    let after = db.connection_tree().await.expect("tree");
    assert_eq!(after, before);
    assert_eq!(after.folders[0].connections[0].id, web.id);
    db.close().await;
}

/// A database from before the connection tree (only the settings applied) gains the tables on
/// its next open and keeps what it held.
#[tokio::test]
async fn the_migration_runs_on_an_existing_database() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(DB_FILE);
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true);
    let mut old = SqliteConnection::connect_with(&options)
        .await
        .expect("connect");
    MIGRATOR.run_to(1, &mut old).await.expect("settings only");
    sqlx::query("INSERT INTO settings (key, value) VALUES ('theme', '\"dark\"')")
        .execute(&mut old)
        .await
        .expect("a setting");
    let tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE name IN ('connections', 'connection_folders')",
    )
    .fetch_one(&mut old)
    .await
    .expect("count");
    assert_eq!(tables, 0, "the old database has no connection tables");
    old.close().await.expect("close");

    let db = Db::open(&path).await.expect("migrates");
    assert_eq!(
        db.get::<String>("theme").await.expect("get").as_deref(),
        Some("dark")
    );
    let folder = db.create_folder(None, "prod").await.expect("folder");
    db.create_connection(Some(folder.id), &ConnectionDraft::new("web", "example.org"))
        .await
        .expect("connection");
    assert_eq!(db.connection_tree().await.expect("tree").folders.len(), 1);
    db.close().await;
}

#[tokio::test]
async fn the_migration_runs_on_an_empty_database() {
    let db = Db::open_in_memory().await.expect("open");
    assert_eq!(
        db.connection_tree().await.expect("tree"),
        ConnectionTree::default()
    );
}

/// A folder's members, a deleted folder's cascade and a deleted script's trigger look rows up by
/// the link column; the unique name indexes cover `ifnull(...)` only, so without migration 0019
/// each of these read the whole table (RC-080-54).
#[tokio::test]
async fn a_lookup_by_the_link_columns_uses_an_index() {
    let db = Db::open_in_memory().await.expect("open");
    let lookups = [
        (
            "EXPLAIN QUERY PLAN SELECT id FROM connection_folders WHERE parent_id = ?",
            "connection_folders_parent",
        ),
        (
            "EXPLAIN QUERY PLAN SELECT id FROM connections WHERE folder_id = ?",
            "connections_folder",
        ),
        (
            "EXPLAIN QUERY PLAN SELECT id FROM connections WHERE login_script_id = ?",
            "connections_login_script",
        ),
        (
            "EXPLAIN QUERY PLAN SELECT id FROM credentials WHERE folder_id = ?",
            "credentials_folder",
        ),
    ];
    for (sql, index) in lookups {
        let plan: Vec<(i64, i64, i64, String)> = sqlx::query_as(sql)
            .bind(1_i64)
            .fetch_all(db.readers())
            .await
            .expect("plan");
        assert!(
            plan.iter().any(|(_, _, _, detail)| detail.contains(index)),
            "{sql}: {plan:?}"
        );
    }
}
