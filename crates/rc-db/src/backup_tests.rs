use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use sqlx::{AssertSqlSafe, Connection as _, SqliteConnection, sqlite::SqliteConnectOptions};

use super::{BackupError, Plan, backup_file, backup_file_name, is_database_file_name};
use crate::{DB_FILE, Db};

async fn file_db(dir: &Path) -> Db {
    Db::open(dir.join(DB_FILE)).await.expect("open")
}

/// A connection of its own to a backup, read-only.
async fn open_backup(path: &Path) -> SqliteConnection {
    let options = SqliteConnectOptions::new().filename(path).read_only(true);
    SqliteConnection::connect_with(&options)
        .await
        .expect("open the backup")
}

/// Every name in `dir`.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn a_backup_is_a_checked_copy_with_the_data() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = file_db(dir.path()).await;
    db.set("panel.left", "C:\\work").await.expect("set");
    let target = dir.path().join("copy.db");

    let report = db.backup_to(&target, false).await.expect("backup");

    assert_eq!(report.path, target);
    assert_eq!(report.size, std::fs::metadata(&target).expect("meta").len());
    let copy = Db::open(&target).await.expect("open the copy");
    assert_eq!(
        copy.get::<String>("panel.left").await.expect("get"),
        Some("C:\\work".to_owned())
    );
    copy.close().await;
    db.close().await;
    // Nothing but the database, its WAL files and the backup.
    assert!(!names(dir.path()).iter().any(|name| name.ends_with(".part")));
}

#[cfg(unix)]
#[tokio::test]
async fn a_backup_is_readable_by_its_owner_only() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().expect("tempdir");
    let db = file_db(dir.path()).await;
    let report = db
        .backup_to(dir.path().join("copy.db"), false)
        .await
        .expect("backup");
    let mode = std::fs::metadata(&report.path)
        .expect("meta")
        .permissions()
        .mode();
    assert_eq!(mode & 0o077, 0, "mode {mode:o}");
    db.close().await;
}

#[tokio::test]
async fn a_folder_gets_a_generated_name_and_a_second_one_a_number() {
    let dir = tempfile::tempdir().expect("tempdir");
    let folder = dir.path().join("backups");
    std::fs::create_dir(&folder).expect("mkdir");
    let db = file_db(dir.path()).await;

    let first = db.backup_to(&folder, false).await.expect("first");
    let second = db.backup_to(&folder, false).await.expect("second");

    let name = first
        .path
        .file_name()
        .expect("name")
        .to_string_lossy()
        .into_owned();
    // rcommander-YYYYMMDD-HHMMSS.db
    assert_eq!(name.len(), "rcommander-20261017-120000.db".len(), "{name}");
    assert!(
        name.starts_with("rcommander-") && name.ends_with(".db"),
        "{name}"
    );
    assert_ne!(first.path, second.path);
    assert!(is_database_file_name(&name));
    db.close().await;
}

#[tokio::test]
async fn an_existing_target_stays_unless_overwriting_is_asked_for() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = file_db(dir.path()).await;
    let target = dir.path().join("taken.db");
    std::fs::write(&target, b"keep me").expect("write");

    let refused = db.backup_to(&target, false).await;
    assert!(
        matches!(refused, Err(BackupError::TargetExists { .. })),
        "{refused:?}"
    );
    assert_eq!(
        refused.err().map(|error| error.code()),
        Some("backup_target_exists")
    );
    assert_eq!(std::fs::read(&target).expect("read"), b"keep me");

    db.backup_to(&target, true).await.expect("overwrite");
    let mut copy = open_backup(&target).await;
    let problems = super::integrity_problems(&mut copy).await.expect("check");
    assert_eq!(problems, None);
    db.close().await;
}

#[tokio::test]
async fn the_database_itself_is_never_a_target() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = file_db(dir.path()).await;
    let refused = db.backup_to(dir.path().join(DB_FILE), true).await;
    assert!(
        matches!(refused, Err(BackupError::TargetIsDatabase { .. })),
        "{refused:?}"
    );
    db.close().await;
}

/// A writer adds a row to two tables in one transaction, over and over, while backups run: each
/// backup is sound and holds as many rows in one table as in the other - one moment's state.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_backup_while_another_task_writes_is_consistent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = file_db(dir.path()).await;
    {
        let mut writer = db.writer().await;
        for table in ["pairs_a", "pairs_b"] {
            sqlx::query(AssertSqlSafe(format!(
                "CREATE TABLE {table} (id INTEGER PRIMARY KEY, body BLOB NOT NULL)"
            )))
            .execute(&mut *writer)
            .await
            .expect("create");
        }
    }
    let stop = Arc::new(AtomicBool::new(false));
    let (busy, stopping) = (db.clone(), Arc::clone(&stop));
    let writing = tokio::spawn(async move {
        let mut pairs = 0_i64;
        while !stopping.load(Ordering::Relaxed) {
            let mut writer = busy.writer().await;
            let mut tx = writer.begin().await.expect("begin");
            for table in ["pairs_a", "pairs_b"] {
                sqlx::query(AssertSqlSafe(format!(
                    "INSERT INTO {table} (body) VALUES (zeroblob(2048))"
                )))
                .execute(&mut *tx)
                .await
                .expect("insert");
            }
            tx.commit().await.expect("commit");
            drop(writer);
            pairs += 1;
            tokio::task::yield_now().await;
        }
        pairs
    });

    let mut seen = Vec::new();
    for round in 0..5 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let target = dir.path().join(format!("during-{round}.db"));
        db.backup_to(&target, false).await.expect("backup");
        let mut copy = open_backup(&target).await;
        assert_eq!(
            super::integrity_problems(&mut copy).await.expect("check"),
            None
        );
        let a: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pairs_a")
            .fetch_one(&mut copy)
            .await
            .expect("count a");
        let b: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pairs_b")
            .fetch_one(&mut copy)
            .await
            .expect("count b");
        assert_eq!(a, b, "round {round}");
        seen.push(a);
        copy.close().await.expect("close");
    }
    tokio::time::sleep(Duration::from_millis(10)).await;
    stop.store(true, Ordering::Relaxed);
    let written = writing.await.expect("writer");
    // The writer went on during the backups and after them.
    assert!(
        seen.iter().all(|count| *count < written),
        "{seen:?} of {written}"
    );
    db.close().await;
}

#[tokio::test]
async fn a_failed_backup_leaves_nothing_behind() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = file_db(dir.path()).await;
    let folder = dir.path().join("out");
    std::fs::create_dir(&folder).expect("mkdir");
    let target = folder.join("copy.db");

    // `VACUUM` refuses to run inside a transaction: the copy fails after its file was made.
    let mut connection = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(dir.path().join(DB_FILE)),
    )
    .await
    .expect("connect");
    sqlx::query("BEGIN")
        .execute(&mut connection)
        .await
        .expect("begin");
    let plan = Plan {
        target: &target,
        database: None,
        overwrite: false,
        suffix: "",
    };
    let failed = super::write_verified(&mut connection, &plan, &|| false).await;
    assert!(matches!(failed, Err(BackupError::Sqlite(_))), "{failed:?}");
    assert_eq!(names(&folder), Vec::<String>::new());

    // A folder that does not exist.
    let missing = db
        .backup_to(dir.path().join("no/such/copy.db"), false)
        .await;
    assert_eq!(
        missing.err().map(|error| error.code()),
        Some("backup_target_unwritable")
    );
    assert!(!dir.path().join("no").exists());
    connection.close().await.expect("close");
    db.close().await;
}

/// The caller stops waiting at once: the copy finishes on its task, is never named, and goes.
#[tokio::test]
async fn a_cancelled_backup_leaves_nothing_at_the_target() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = file_db(dir.path()).await;
    let folder = dir.path().join("out");
    std::fs::create_dir(&folder).expect("mkdir");
    let target = folder.join("copy.db");

    let cancelled = tokio::time::timeout(Duration::ZERO, db.backup_to(&target, false)).await;
    assert!(cancelled.is_err(), "the backup finished within no time");

    // The task ends on its own; wait until its temporary file is gone.
    for _ in 0..200 {
        if names(&folder).is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(names(&folder), Vec::<String>::new());
    assert!(!target.exists());
    db.close().await;
}

#[tokio::test]
async fn a_file_is_backed_up_beside_an_open_database() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = file_db(dir.path()).await;
    db.set("k", &7).await.expect("set");
    let target = dir.path().join("cli.db");

    // The window keeps the database open; the command line backs it up through its own
    // connection.
    let report = backup_file(dir.path().join(DB_FILE), &target, false)
        .await
        .expect("backup");
    let copy = Db::open(&report.path).await.expect("open the copy");
    assert_eq!(copy.get::<i32>("k").await.expect("get"), Some(7));
    copy.close().await;

    let missing = backup_file(dir.path().join("none.db"), dir.path().join("x.db"), false).await;
    assert_eq!(
        missing.err().map(|error| error.code()),
        Some("backup_no_database")
    );
    assert!(!dir.path().join("none.db").exists());
    db.close().await;
}

#[tokio::test]
async fn the_backup_folder_is_beside_the_database_and_made_on_demand() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = file_db(dir.path()).await;
    let report = db.backup_to_backup_dir().await.expect("backup");
    assert_eq!(
        report.path.parent(),
        Some(dir.path().join("backups").as_path())
    );

    let memory = Db::open_in_memory().await.expect("memory");
    assert_eq!(
        memory
            .backup_to_backup_dir()
            .await
            .err()
            .map(|error| error.code()),
        Some("backup_in_memory")
    );
    db.close().await;
}

#[test]
fn database_file_names_are_recognised() {
    assert_eq!(
        backup_file_name("20261017-120000"),
        "rcommander-20261017-120000.db"
    );
    for name in [
        "rcommander.db",
        "rcommander.db-wal",
        "rcommander.db.restore",
        "rcommander-20261017-120000.db",
        "RCommander-20261017-120000-before-restore.DB",
        ".copy.db.42-0.part",
        ".rcommander-20261017-120000.db.42-0.part",
    ] {
        if name.starts_with(".copy") {
            assert!(!is_database_file_name(name), "{name}");
        } else {
            assert!(is_database_file_name(name), "{name}");
        }
    }
    for name in ["notes.db", "rcommander.exe", "readme.md"] {
        assert!(!is_database_file_name(name), "{name}");
    }
}

#[test]
fn every_code_is_listed() {
    let error = BackupError::Interrupted;
    assert!(BackupError::ALL_CODES.contains(&error.code()));
    let mut codes = BackupError::ALL_CODES.to_vec();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), BackupError::ALL_CODES.len());
}
