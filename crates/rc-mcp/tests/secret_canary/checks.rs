//! Where the sweep looks besides the answers: every log line at every level from every crate,
//! every file below a folder, and every value of every table of the database - the tables read
//! from `sqlite_master`, so a table a later migration adds is read without a change here.

use std::{
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

use sqlx::{AssertSqlSafe, Connection as _, SqliteConnection};
use tracing_subscriber::util::SubscriberInitExt as _;

use crate::encodings::Needles;

/// Everything every log line wrote, at every level and from every crate, `log` records included.
#[derive(Clone, Default)]
pub struct Log(Arc<Mutex<Vec<u8>>>);

impl Log {
    pub fn bytes(&self) -> Vec<u8> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl io::Write for Log {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The process-wide subscriber, writing into the returned [`Log`].
pub fn capture_logs() -> Log {
    let log = Log::default();
    let writer = log.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    // `try_init` also bridges the `log` facade (russh logs through it), as the app does.
    subscriber.try_init().expect("the only subscriber");
    log
}

/// Every file below `dir`.
fn files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("readable") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            files(&path, found);
        } else {
            found.push(path);
        }
    }
}

/// The files below `dir` that carry a needle, with the needles.
pub fn leaking_files(dir: &Path, needles: &Needles) -> Vec<(PathBuf, Vec<String>)> {
    let mut found = Vec::new();
    files(dir, &mut found);
    found
        .into_iter()
        .filter_map(|path| {
            let leaked = needles.found(&std::fs::read(&path).unwrap_or_default());
            (!leaked.is_empty()).then_some((path, leaked))
        })
        .collect()
}

/// What reading every table found.
pub struct TableScan {
    /// Every table of the database, by name.
    pub tables: Vec<String>,
    /// The tables and columns that carry a needle, with the needles.
    pub leaks: Vec<(String, Vec<String>)>,
}

/// Every value of every column of every table in the database file `file`, as the bytes SQLite
/// holds (`CAST(... AS BLOB)`: text as it is stored, numbers as their text, blobs as they are),
/// searched for the needles.
pub async fn scan_tables(file: &Path, needles: &Needles) -> TableScan {
    let url = format!("sqlite://{}?mode=ro", file.display());
    let mut connection = SqliteConnection::connect(&url).await.expect("opens");
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .fetch_all(&mut connection)
            .await
            .expect("the tables");
    let mut leaks = Vec::new();
    for table in &tables {
        let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info(?)")
            .bind(table.as_str())
            .fetch_all(&mut connection)
            .await
            .expect("the columns");
        for column in columns {
            let sql = format!(
                "SELECT CAST({} AS BLOB) FROM {}",
                quoted(&column),
                quoted(table)
            );
            let values: Vec<Option<Vec<u8>>> = sqlx::query_scalar(AssertSqlSafe(sql))
                .fetch_all(&mut connection)
                .await
                .expect("the values");
            let mut bytes = Vec::new();
            for value in values.into_iter().flatten() {
                bytes.extend_from_slice(&value);
                bytes.push(b'\n');
            }
            let leaked = needles.found(&bytes);
            if !leaked.is_empty() {
                leaks.push((format!("{table}.{column}"), leaked));
            }
        }
    }
    connection.close().await.expect("closes");
    TableScan { tables, leaks }
}

/// `name` as an SQL identifier.
fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// A copy of the database file `file` at `copy` with a table of its own holding `planted`: the
/// table scan must find what no migration put there.
pub async fn plant_table(file: &Path, copy: &Path, planted: &str) {
    std::fs::copy(file, copy).expect("copied");
    let url = format!("sqlite://{}", copy.display());
    let mut connection = SqliteConnection::connect(&url).await.expect("opens");
    sqlx::query("CREATE TABLE planted_by_the_sweep (note TEXT)")
        .execute(&mut connection)
        .await
        .expect("created");
    sqlx::query("INSERT INTO planted_by_the_sweep (note) VALUES (?)")
        .bind(planted)
        .execute(&mut connection)
        .await
        .expect("planted");
    connection.close().await.expect("closes");
}
