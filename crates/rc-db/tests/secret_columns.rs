//! Every encrypted column of the schema is a secret column, and every secret column exists
//! (RC-080-03).
//!
//! A column that holds ciphertext is named `<name>_enc`. `SECRET_COLUMNS` is the list a change of
//! the master password re-encrypts and the removal of the vault checks: a migration that adds an
//! `_enc` column without a row there leaves its values under the old key after a password change,
//! unreadable for good. This test reads the schema the migrations build and holds it against the
//! list in both directions.

use std::collections::BTreeSet;

use rc_db::{DB_FILE, Db, SECRET_COLUMNS};
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};

/// The suffix of every column that holds a vault value.
const ENCRYPTED: &str = "_enc";

#[tokio::test]
async fn every_encrypted_column_is_a_secret_column_and_back() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(DB_FILE);
    Db::open(&path).await.expect("migrated").close().await;

    let options = SqliteConnectOptions::new().filename(&path).read_only(true);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("opens");
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_all(&mut connection)
    .await
    .expect("tables");
    let mut columns = BTreeSet::new();
    for table in &tables {
        let names: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info(?1)")
            .bind(table)
            .fetch_all(&mut connection)
            .await
            .expect("columns");
        for name in names {
            columns.insert((table.clone(), name));
        }
    }
    let _ = connection.close().await;

    let encrypted: BTreeSet<(String, String)> = columns
        .iter()
        .filter(|(_, column)| column.ends_with(ENCRYPTED))
        .cloned()
        .collect();
    let listed: BTreeSet<(String, String)> = SECRET_COLUMNS
        .iter()
        .map(|secret| (secret.table.to_owned(), secret.column.to_owned()))
        .collect();
    assert_eq!(
        encrypted, listed,
        "an `_enc` column is missing from rc_db::SECRET_COLUMNS, or a listed one from the schema"
    );
    for secret in SECRET_COLUMNS {
        assert!(
            columns.contains(&(secret.table.to_owned(), secret.row_id.to_owned())),
            "{}.{} has no row id column {}",
            secret.table,
            secret.column,
            secret.row_id
        );
    }
}
