use rc_db::{DB_FILE, Db};

use crate::test_support;
use crate::{Command, Engine, Outcome};

/// An engine over a database file in a temporary data directory.
async fn engine_with_file_db() -> (tempfile::TempDir, Engine) {
    let data = tempfile::tempdir().expect("temp dir");
    let engine = test_support::headless(data.path()).await;
    let db = Db::open(data.path().join(DB_FILE)).await.expect("db");
    assert!(engine.attach_db(db));
    (data, engine)
}

#[tokio::test]
async fn a_backup_goes_into_the_backup_folder_beside_the_database() {
    let (data, engine) = engine_with_file_db().await;
    let outcome = engine
        .dispatch_mcp(Command::BackupDatabase)
        .await
        .expect("backup");
    let Outcome::DatabaseBackup(report) = outcome else {
        panic!("expected a backup, got {outcome:?}");
    };
    assert_eq!(
        report.path.parent(),
        Some(data.path().join(rc_db::BACKUP_DIR).as_path())
    );
    assert!(report.size > 0);
}

#[tokio::test]
async fn a_chosen_path_and_a_restore_are_for_the_window_only() {
    let (data, engine) = engine_with_file_db().await;
    let target = data.path().join("chosen.db");
    for command in [
        Command::BackupDatabaseTo(crate::BackupToRequest {
            path: target.to_string_lossy().into_owned(),
            overwrite: true,
        }),
        Command::RestoreDatabase {
            path: target.to_string_lossy().into_owned(),
        },
    ] {
        let refused = engine.dispatch_mcp(command).await.expect_err("gui only");
        assert_eq!(refused.code(), "gui_only");
    }
    assert!(!target.exists());

    // The window runs both.
    let outcome = engine
        .dispatch(Command::BackupDatabaseTo(crate::BackupToRequest {
            path: target.to_string_lossy().into_owned(),
            overwrite: false,
        }))
        .await
        .expect("backup");
    assert!(matches!(outcome, Outcome::DatabaseBackup(_)), "{outcome:?}");
    let again = engine
        .dispatch(Command::BackupDatabaseTo(crate::BackupToRequest {
            path: target.to_string_lossy().into_owned(),
            overwrite: false,
        }))
        .await
        .expect_err("exists");
    assert_eq!(again.code(), "backup_target_exists");

    let staged = engine
        .dispatch(Command::RestoreDatabase {
            path: target.to_string_lossy().into_owned(),
        })
        .await
        .expect("restore");
    let Outcome::RestoreStaged(report) = staged else {
        panic!("expected a staged restore, got {staged:?}");
    };
    assert!(report.staged.is_file());
}

#[tokio::test]
async fn without_a_database_there_is_nothing_to_back_up() {
    let (_root, engine) = test_support::engine(&[]).await;
    let error = engine
        .dispatch(Command::BackupDatabase)
        .await
        .expect_err("no database");
    assert_eq!(error.code(), "database_unavailable");
}

/// RC-080-42, the owner's decision of 2026-10-28: after a backup the folder keeps the newest
/// `backup.keep` backups under a generated name; one a person named stays.
#[tokio::test]
async fn a_backup_keeps_the_newest_automatic_backups_and_every_named_one() {
    let (data, engine) = engine_with_file_db().await;
    let folder = data.path().join(rc_db::BACKUP_DIR);
    std::fs::create_dir_all(&folder).expect("mkdir");
    for old in [
        "rcommander-20200101-000000.db",
        "rcommander-20200102-000000.db",
        "rcommander-20200103-000000.db",
    ] {
        std::fs::write(folder.join(old), b"old").expect("write");
    }
    std::fs::write(folder.join("before-the-upgrade.db"), b"mine").expect("write");
    // Protected: stored as the settings dialog stores it, not through `settings.set`.
    engine
        .db()
        .expect("a database")
        .set(crate::settings::BACKUP_KEEP_KEY, &2_u64)
        .await
        .expect("set");
    let Outcome::DatabaseBackup(report) = engine
        .dispatch(Command::BackupDatabase)
        .await
        .expect("backup")
    else {
        panic!("a backup");
    };
    let mut left: Vec<String> = std::fs::read_dir(&folder)
        .expect("list")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    left.sort();
    let new = report
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .expect("a name");
    let mut expected = vec![
        "before-the-upgrade.db".to_owned(),
        "rcommander-20200103-000000.db".to_owned(),
        new,
    ];
    expected.sort();
    assert_eq!(left, expected);
}
