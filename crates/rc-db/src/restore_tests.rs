use std::path::Path;

use sqlx::{Connection as _, SqliteConnection, sqlite::SqliteConnectOptions};

use super::{
    RESTORE_DIGEST_KEY, apply_pending_restore, file_digest, is_rcommander_database,
    looks_like_rcommander_database, rejected_restore_path, staged_restore_path,
};
use crate::{BackupError, DB_FILE, Db};

/// A database in `dir` with a setting, and a backup of it at `<dir>/backup.db`.
async fn with_backup(dir: &Path) -> (Db, std::path::PathBuf) {
    let db = Db::open(dir.join(DB_FILE)).await.expect("open");
    db.set("state", "backed up").await.expect("set");
    let backup = dir.join("backup.db");
    db.backup_to(&backup, false).await.expect("backup");
    (db, backup)
}

/// Runs `sql` on the file at `path` through a connection of its own.
async fn alter(path: &Path, sql: &'static str) {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("connect");
    sqlx::raw_sql(sql)
        .execute(&mut connection)
        .await
        .expect("alter");
    connection.close().await.expect("close");
}

/// Nothing was staged and no temporary file is left beside the database.
fn nothing_staged(dir: &Path) {
    assert!(!staged_restore_path(&dir.join(DB_FILE)).exists());
    let parts: Vec<String> = std::fs::read_dir(dir)
        .expect("read dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".part"))
        .collect();
    assert_eq!(parts, Vec::<String>::new());
}

#[tokio::test]
async fn a_backup_from_a_newer_version_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (db, backup) = with_backup(dir.path()).await;
    alter(
        &backup,
        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) \
         VALUES (9999, 'from the future', 1, x'00', 0)",
    )
    .await;

    let refused = db.restore_from(&backup).await;
    match refused {
        Err(BackupError::RestoreNewer { found, known, .. }) => {
            assert_eq!(found, 9999);
            assert!(known < found);
        }
        other => panic!("expected restore_newer, got {other:?}"),
    }
    nothing_staged(dir.path());
    db.close().await;
}

#[tokio::test]
async fn a_foreign_database_or_a_changed_migration_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (db, backup) = with_backup(dir.path()).await;

    let foreign = dir.path().join("photos.sqlite");
    alter(
        &foreign,
        "CREATE TABLE photos (id INTEGER PRIMARY KEY, name TEXT)",
    )
    .await;
    let refused = db.restore_from(&foreign).await;
    assert_eq!(
        refused.err().map(|error| error.code()),
        Some("restore_not_rcommander")
    );

    alter(
        &backup,
        "UPDATE _sqlx_migrations SET checksum = x'00' WHERE version = 1",
    )
    .await;
    let refused = db.restore_from(&backup).await;
    assert_eq!(
        refused.err().map(|error| error.code()),
        Some("restore_not_rcommander")
    );
    nothing_staged(dir.path());
    db.close().await;
}

#[tokio::test]
async fn a_file_that_is_no_database_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Db::open(dir.path().join(DB_FILE)).await.expect("open");
    let text = dir.path().join("notes.db");
    std::fs::write(
        &text,
        "not a database, only text that is long enough for a header",
    )
    .expect("write");
    let refused = db.restore_from(&text).await;
    assert_eq!(
        refused.err().map(|error| error.code()),
        Some("restore_not_database")
    );
    let missing = db.restore_from(dir.path().join("none.db")).await;
    assert_eq!(
        missing.err().map(|error| error.code()),
        Some("restore_not_database")
    );
    nothing_staged(dir.path());
    db.close().await;
}

/// Backup, a change, restore, restart: the backed-up state is back, and the state it replaced
/// is in the safety backup.
#[tokio::test]
async fn a_staged_restore_replaces_the_database_at_the_next_start() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(DB_FILE);
    let (db, backup) = with_backup(dir.path()).await;
    db.set("state", "changed later").await.expect("set");

    let report = db.restore_from(&backup).await.expect("restore");
    assert_eq!(report.checked.path, backup);
    assert!(report.checked.schema_version > 0);
    assert!(!report.checked.has_vault);
    assert_eq!(report.staged, staged_restore_path(&path));
    assert!(report.staged.is_file());
    let safety = report
        .safety_backup
        .path
        .file_name()
        .expect("name")
        .to_string_lossy()
        .into_owned();
    assert!(safety.ends_with("-before-restore.db"), "{safety}");
    // Until the restart the running program keeps its state.
    assert_eq!(
        db.get::<String>("state").await.expect("get").as_deref(),
        Some("changed later")
    );
    // The digest of the staged file waits inside the database it replaces.
    let stored = db.get::<String>(RESTORE_DIGEST_KEY).await.expect("get");
    let staged = file_digest(&report.staged).await.expect("digest");
    assert_eq!(stored, Some(staged.to_hex().to_string()));
    db.close().await;

    assert!(apply_pending_restore(&path).await.expect("swap"));
    assert!(!staged_restore_path(&path).exists());
    let reopened = Db::open(&path).await.expect("reopen");
    assert_eq!(
        reopened
            .get::<String>(RESTORE_DIGEST_KEY)
            .await
            .expect("get"),
        None,
        "the digest is cleared after the swap"
    );
    assert_eq!(
        reopened
            .get::<String>("state")
            .await
            .expect("get")
            .as_deref(),
        Some("backed up")
    );
    reopened.close().await;

    let before = Db::open(&report.safety_backup.path)
        .await
        .expect("open safety");
    assert_eq!(
        before.get::<String>("state").await.expect("get").as_deref(),
        Some("changed later")
    );
    before.close().await;

    // No restore waiting: nothing happens.
    assert!(!apply_pending_restore(&path).await.expect("nothing to swap"));
}

#[tokio::test]
async fn database_content_is_recognised() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (db, backup) = with_backup(dir.path()).await;
    assert!(is_rcommander_database(&backup).await);

    let foreign = dir.path().join("other.sqlite");
    alter(&foreign, "CREATE TABLE things (id INTEGER PRIMARY KEY)").await;
    assert!(!is_rcommander_database(&foreign).await);

    let text = dir.path().join("readme.txt");
    std::fs::write(&text, "hello").expect("write");
    assert!(!is_rcommander_database(&text).await);
    assert!(!is_rcommander_database(&dir.path().join("missing")).await);

    // The same answers without SQLite, for the engine's read guard.
    assert!(looks_like_rcommander_database(&backup));
    assert!(!looks_like_rcommander_database(&foreign));
    assert!(!looks_like_rcommander_database(&text));
    assert!(!looks_like_rcommander_database(&dir.path().join("missing")));
    assert!(!looks_like_rcommander_database(dir.path()));
    db.close().await;
}

/// The setting the database at `path` holds under `state`.
async fn state_of(path: &Path) -> Option<String> {
    let db = Db::open(path).await.expect("open");
    let state = db.get::<String>("state").await.expect("get");
    db.close().await;
    state
}

/// A database in `dir` holding "current" and, when given, `digest` as the restore digest, and
/// beside it a valid rCommander database of another state staged as `<database>.restore` - not
/// by `restore_from`, as a write tool or another program would leave it.
async fn planted(dir: &Path, digest: Option<&str>) -> std::path::PathBuf {
    let path = dir.join(DB_FILE);
    let (db, backup) = with_backup(dir).await;
    db.set("state", "current").await.expect("set");
    if let Some(digest) = digest {
        db.set(RESTORE_DIGEST_KEY, digest).await.expect("digest");
    }
    db.close().await;
    std::fs::rename(&backup, staged_restore_path(&path)).expect("plant");
    path
}

/// Asserts that the restore at `path` was refused and set aside, and that the database stays.
async fn set_aside_and_kept(path: &Path, case: &str) {
    match apply_pending_restore(path).await {
        Err(BackupError::RestoreRejected { set_aside, .. }) => {
            assert_eq!(set_aside, rejected_restore_path(path), "{case}");
        }
        other => panic!("expected restore_rejected for {case}, got {other:?}"),
    }
    assert!(!staged_restore_path(path).exists(), "{case}");
    assert!(rejected_restore_path(path).is_file(), "{case}");
    assert_eq!(state_of(path).await.as_deref(), Some("current"), "{case}");
    let db = Db::open(path).await.expect("open");
    let left = db.get::<String>(RESTORE_DIGEST_KEY).await.expect("get");
    db.close().await;
    assert_eq!(left, None, "{case}: the digest is cleared");
    // The next start has nothing to do.
    assert!(
        !apply_pending_restore(path).await.expect("nothing left"),
        "{case}"
    );
}

/// RC-080-09: a `.restore` file the window's restore did not prepare never replaces the
/// database at the start, even with a self-made mark beside it holding its true digest - the
/// digest that counts is inside the database, where no write tool reaches; a wrong or broken
/// digest there does not match either. The file is set aside and the database stays.
#[tokio::test]
async fn a_restore_file_the_window_did_not_prepare_is_set_aside() {
    let wrong = "0".repeat(64);
    for (case, stored) in [
        ("no digest", None),
        ("a wrong digest", Some(wrong.as_str())),
        ("a broken digest", Some("not a digest")),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = planted(dir.path(), stored).await;
        // Self-made marks beside the database, as a write tool could leave them.
        let digest = file_digest(&staged_restore_path(&path))
            .await
            .expect("digest");
        for suffix in [".restore-mark", ".restore.digest", ".restore.sha256"] {
            let mut name = path.clone().into_os_string();
            name.push(suffix);
            std::fs::write(name, digest.to_hex().as_str()).expect("forged mark");
        }
        set_aside_and_kept(&path, case).await;
    }
}

/// A file staged by `restore_from` and changed afterwards no longer has its digest.
#[tokio::test]
async fn a_staged_restore_changed_after_its_check_is_set_aside() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(DB_FILE);
    let (db, backup) = with_backup(dir.path()).await;
    db.set("state", "current").await.expect("set");
    db.restore_from(&backup).await.expect("restore");
    db.close().await;
    // Another rCommander database, swapped in under the staged name.
    let other = dir.path().join("other.db");
    let foreign = Db::open(&other).await.expect("open other");
    foreign.set("state", "foreign").await.expect("set");
    foreign
        .backup_to(staged_restore_path(&path), true)
        .await
        .expect("swap");
    foreign.close().await;

    set_aside_and_kept(&path, "changed after the check").await;
}

/// A file with the stored digest that is no sound rCommander database fails the check again
/// and is set aside.
#[tokio::test]
async fn a_file_with_its_digest_that_fails_the_check_is_set_aside() {
    let dir = tempfile::tempdir().expect("tempdir");
    let text = dir.path().join("text.bin");
    std::fs::write(
        &text,
        "not a database, only text that is long enough for a header",
    )
    .expect("write");
    let digest = file_digest(&text).await.expect("digest").to_hex();
    let path = planted(dir.path(), Some(digest.as_str())).await;
    std::fs::rename(&text, staged_restore_path(&path)).expect("stage");

    set_aside_and_kept(&path, "no database").await;
}

/// A folder under the staged name is nobody's restore: left alone, and the database stays.
#[tokio::test]
async fn a_folder_under_the_restore_name_is_not_touched() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(DB_FILE);
    let db = Db::open(&path).await.expect("open");
    db.set("state", "current").await.expect("set");
    db.close().await;
    let folder = staged_restore_path(&path);
    std::fs::create_dir(&folder).expect("mkdir");
    std::fs::write(folder.join("inside.txt"), "x").expect("write");

    assert!(!apply_pending_restore(&path).await.expect("nothing to swap"));
    assert!(folder.join("inside.txt").is_file());
    assert!(!rejected_restore_path(&path).exists());
    assert_eq!(state_of(&path).await.as_deref(), Some("current"));
}
