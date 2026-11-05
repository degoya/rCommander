use std::path::Path;

use sqlx::{Connection as _, SqliteConnection, sqlite::SqliteConnectOptions};

use super::{Difference, difference, shown_name};
use crate::{DB_FILE, Db, MIGRATOR};

/// The last migration this build has.
fn latest() -> i64 {
    MIGRATOR
        .iter()
        .filter(|migration| migration.migration_type.is_up_migration())
        .map(|migration| migration.version)
        .max()
        .expect("migrations")
}

/// A database the migrations made in `dir`, `sql` run on it, and the difference found.
async fn differs_after(dir: &Path, sql: &'static str) -> Option<Difference> {
    let path = dir.join(DB_FILE);
    let db = Db::open(&path).await.expect("open");
    db.close().await;
    let options = SqliteConnectOptions::new().filename(&path);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("connect");
    if !sql.is_empty() {
        sqlx::raw_sql(sql)
            .execute(&mut connection)
            .await
            .expect("alter");
    }
    let found = difference(&mut connection, latest())
        .await
        .expect("compared");
    connection.close().await.expect("close");
    found
}

fn foreign(kind: &str, name: &str) -> Option<Difference> {
    Some(Difference::Foreign {
        kind: kind.to_owned(),
        name: name.to_owned(),
    })
}

/// RC-080-68: a database the migrations made is exactly their schema - SQLite's own objects
/// (statistics, automatic indexes) left out.
#[tokio::test]
async fn the_migrations_schema_has_no_difference() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert_eq!(differs_after(dir.path(), "").await, None);
    let dir = tempfile::tempdir().expect("tempdir");
    assert_eq!(differs_after(dir.path(), "ANALYZE").await, None);
}

/// RC-080-68: a trigger, a view or a table of the file's own is foreign, and so is the
/// migrations' own trigger with another body; a missing index is not this schema either.
#[tokio::test]
async fn an_object_of_the_files_own_is_foreign() {
    for (sql, expected) in [
        (
            "CREATE TRIGGER leak AFTER UPDATE ON settings BEGIN \
             INSERT INTO settings (key, value) VALUES ('copy', NEW.value); END",
            foreign("trigger", "leak"),
        ),
        (
            "CREATE VIEW everything AS SELECT * FROM settings",
            foreign("view", "everything"),
        ),
        (
            "CREATE TABLE extra (id INTEGER PRIMARY KEY)",
            foreign("table", "extra"),
        ),
        (
            "DROP TRIGGER login_scripts_unlink; \
             CREATE TRIGGER login_scripts_unlink AFTER DELETE ON login_scripts BEGIN \
             DELETE FROM connections; END",
            foreign("trigger", "login_scripts_unlink"),
        ),
        (
            "DROP INDEX error_log_recorded_at",
            Some(Difference::Missing),
        ),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(differs_after(dir.path(), sql).await, expected, "{sql}");
    }
}

#[test]
fn a_shown_name_is_short_and_plain() {
    assert_eq!(shown_name("a\nb\u{1b}c"), "abc");
    assert_eq!(shown_name(&"x".repeat(200)).chars().count(), 64);
}
