use super::{DB_FILE, Db, MIGRATOR};

/// How many migrations this build carries; every open applies all of them.
fn shipped() -> i64 {
    i64::try_from(MIGRATOR.iter().count()).expect("a handful of migrations")
}

async fn applied_migrations(db: &Db) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success")
        .fetch_one(db.readers())
        .await
        .expect("count migrations")
}

#[tokio::test]
async fn open_creates_the_directory_and_the_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("nested/data").join(DB_FILE);
    let db = Db::open(&path).await.expect("open");
    assert!(path.is_file());
    assert_eq!(applied_migrations(&db).await, shipped());
    db.close().await;
}

#[tokio::test]
async fn reopening_migrates_nothing_twice_and_keeps_the_data() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(DB_FILE);

    let db = Db::open(&path).await.expect("first open");
    db.set("panel.left", "C:\\").await.expect("set");
    db.close().await;

    let db = Db::open(&path).await.expect("second open");
    assert_eq!(applied_migrations(&db).await, shipped());
    assert_eq!(
        db.get::<String>("panel.left").await.expect("get"),
        Some("C:\\".to_owned())
    );
    db.close().await;
}

#[tokio::test]
async fn the_file_is_in_wal_mode_with_foreign_keys() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Db::open(dir.path().join(DB_FILE)).await.expect("open");
    let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(db.readers())
        .await
        .expect("journal mode");
    assert_eq!(mode, "wal");
    let keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(db.readers())
        .await
        .expect("foreign keys");
    assert_eq!(keys, 1);
    db.close().await;
}

#[tokio::test]
async fn in_memory_databases_are_migrated_and_separate() {
    let first = Db::open_in_memory().await.expect("open");
    let second = Db::open_in_memory().await.expect("open");
    assert_eq!(applied_migrations(&first).await, shipped());

    first.set("k", &1).await.expect("set");
    assert_eq!(first.clone().get::<i32>("k").await.expect("get"), Some(1));
    assert_eq!(second.get::<i32>("k").await.expect("get"), None);
}

#[test]
fn the_default_path_is_rcommander_db_in_the_data_directory() {
    // A home directory is all `directories` needs; the build machines have one.
    let path = Db::default_path().expect("data directory");
    assert!(path.ends_with(DB_FILE), "{}", path.display());
    assert!(
        path.to_string_lossy().contains("rCommander")
            || path.to_string_lossy().contains("rcommander"),
        "{}",
        path.display()
    );
}

use sqlx::{
    Connection as _, SqliteConnection,
    sqlite::{SqliteAutoVacuum, SqliteConnectOptions},
};

use super::{AUTO_VACUUM_INCREMENTAL, switch_to_incremental_vacuum};

async fn auto_vacuum(db: &Db) -> i64 {
    sqlx::query_scalar("PRAGMA auto_vacuum")
        .fetch_one(db.readers())
        .await
        .expect("auto_vacuum")
}

#[tokio::test]
async fn a_new_file_is_created_with_incremental_vacuum() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Db::open(dir.path().join(DB_FILE)).await.expect("open");
    assert_eq!(auto_vacuum(&db).await, AUTO_VACUUM_INCREMENTAL);
    db.close().await;
}

/// A file from before RC-010-11 (`auto_vacuum = NONE`, tables in it) is switched on its first
/// open, keeps its data, and is not rewritten again on the next.
#[tokio::test]
async fn an_existing_file_without_auto_vacuum_is_switched_once() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(DB_FILE);
    let plain = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true);
    let mut old = SqliteConnection::connect_with(&plain).await.expect("old");
    sqlx::query("CREATE TABLE older (value TEXT NOT NULL)")
        .execute(&mut old)
        .await
        .expect("table");
    sqlx::query("INSERT INTO older (value) VALUES ('kept')")
        .execute(&mut old)
        .await
        .expect("row");
    let before: i64 = sqlx::query_scalar("PRAGMA auto_vacuum")
        .fetch_one(&mut old)
        .await
        .expect("auto_vacuum");
    assert_eq!(before, 0, "the old file has no auto_vacuum");
    old.close().await.expect("close");

    let db = Db::open(&path).await.expect("open");
    assert_eq!(auto_vacuum(&db).await, AUTO_VACUUM_INCREMENTAL);
    let kept: String = sqlx::query_scalar("SELECT value FROM older")
        .fetch_one(db.readers())
        .await
        .expect("row survives");
    assert_eq!(kept, "kept");
    db.close().await;

    let mut again = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(&path)
            .auto_vacuum(SqliteAutoVacuum::Incremental),
    )
    .await
    .expect("reopen");
    assert!(
        !switch_to_incremental_vacuum(&mut again).await,
        "a switched file is not rewritten again"
    );
    again.close().await.expect("close");
}

#[tokio::test]
async fn the_switch_runs_only_on_a_file_that_needs_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let options = SqliteConnectOptions::new()
        .filename(dir.path().join("plain.db"))
        .create_if_missing(true);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("open");
    sqlx::query("CREATE TABLE t (x INTEGER)")
        .execute(&mut connection)
        .await
        .expect("table");
    sqlx::query("PRAGMA auto_vacuum = INCREMENTAL")
        .execute(&mut connection)
        .await
        .expect("pragma");
    assert!(switch_to_incremental_vacuum(&mut connection).await);
    assert!(!switch_to_incremental_vacuum(&mut connection).await);
    connection.close().await.expect("close");
}

/// RC-080-11 (V5): every connection overwrites what it deletes, so a replaced ciphertext does not
/// stay in a free page of the file.
#[tokio::test]
async fn every_connection_runs_with_secure_delete() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Db::open(dir.path().join(DB_FILE)).await.expect("open");
    let reader: i64 = sqlx::query_scalar("PRAGMA secure_delete")
        .fetch_one(db.readers())
        .await
        .expect("reader");
    let writer: i64 = sqlx::query_scalar("PRAGMA secure_delete")
        .fetch_one(&mut *db.writer().await)
        .await
        .expect("writer");
    assert_eq!((reader, writer), (1, 1));
    db.close().await;
}
