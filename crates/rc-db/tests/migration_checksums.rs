//! An applied migration stays as it is.
//!
//! sqlx stores the SHA-384 of every migration file it applies, in `_sqlx_migrations.checksum`,
//! and compares it on every open. One changed byte - a comment is enough - and every database
//! that applied the file refuses to open with "migration N was previously applied but has been
//! modified". (rDownloader lost a start to one comment line; this test is the rule as code.)
//!
//! `crates/rc-db/migrations.sha384` pins every migration's checksum, one line per file, in plain
//! `sha384sum` format. Only `scripts/migration-pin.sh` writes it, and it only appends.

use std::{borrow::Cow, collections::BTreeMap};

use sqlx::{
    Connection, SqliteConnection,
    migrate::{Migration, MigrationType},
};

/// The pin file, compiled in so that editing it rebuilds this test.
const PINS: &str = include_str!("../migrations.sha384");

/// The one command that adds a pin.
const PIN_COMMAND: &str = "scripts/migration-pin.sh";

/// One line of the pin file.
struct Pin {
    checksum: String,
    file: String,
}

/// Parses pin-file text, keyed by migration number.
fn parse_pins(text: &str) -> BTreeMap<i64, Pin> {
    let mut pins = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        let line_number = index + 1;
        let (checksum, file) = line.split_once("  ").unwrap_or_else(|| {
            panic!("migrations.sha384 line {line_number} is not `<sha384>  <file>`: {line:?}")
        });
        assert!(
            checksum.len() == 96 && checksum.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "migrations.sha384 line {line_number} carries no SHA-384: {line:?}"
        );
        let version: i64 = file
            .split_once('_')
            .and_then(|(number, _)| number.parse().ok())
            .unwrap_or_else(|| {
                panic!("migrations.sha384 line {line_number} names no numbered file: {line:?}")
            });
        let pin = Pin {
            checksum: checksum.to_ascii_lowercase(),
            file: file.to_owned(),
        };
        assert!(
            pins.insert(version, pin).is_none(),
            "migrations.sha384 pins migration {version} twice"
        );
    }
    pins
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The file name sqlx read the migration from; it derives the description by replacing `_`.
fn file_name(migration: &Migration) -> String {
    format!(
        "{:04}_{}.sql",
        migration.version,
        migration.description.replace(' ', "_")
    )
}

/// Everything that would stop a database from opening, one message each.
fn problems<'a>(
    migrations: impl IntoIterator<Item = &'a Migration>,
    mut pins: BTreeMap<i64, Pin>,
) -> Vec<String> {
    let mut problems = Vec::new();
    for migration in migrations {
        let file = file_name(migration);
        let Some(pin) = pins.remove(&migration.version) else {
            problems.push(format!(
                "{file} has no pinned checksum. Pin it now, while no database has applied it \
                 yet, and commit the pin with the migration:\n    {PIN_COMMAND} {file}"
            ));
            continue;
        };
        if pin.file != file {
            problems.push(format!(
                "migration {} is pinned as {} but the file is {file}. Keep the name it was \
                 pinned under.",
                migration.version, pin.file
            ));
        }
        let actual = hex(&migration.checksum);
        if pin.checksum != actual {
            problems.push(format!(
                "{file} changed after it was pinned (pinned {}, now {actual}). Every database \
                 that applied it refuses to open: \"migration {} was previously applied but has \
                 been modified\" - even over a comment. Restore the file's bytes \
                 (`git log -- crates/rc-db/migrations/{file}`) and put the change in a new \
                 migration. Never move the pin.",
                pin.checksum, migration.version
            ));
        }
    }
    for (version, pin) in pins {
        problems.push(format!(
            "{} is pinned but no longer exists. Every database that applied it refuses to open: \
             \"migration {version} was previously applied but is missing in the resolved \
             migrations\". Restore the file.",
            pin.file
        ));
    }
    problems
}

/// Every migration has a pin, and every pin still matches the file sqlx compiles in - the very
/// checksum an open writes into and compares against `_sqlx_migrations`.
#[test]
fn every_migration_is_pinned_and_unchanged() {
    let problems = problems(sqlx::migrate!().iter(), parse_pins(PINS));
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n\n"));
}

/// A migration as the migrator would read it from `NNNN_<description>.sql`.
fn migration(version: i64, description: &'static str, sql: &'static str) -> Migration {
    Migration::new(
        version,
        Cow::Borrowed(description),
        MigrationType::Simple,
        sqlx::SqlSafeStr::into_sql_str(sql),
        false,
    )
}

/// The line `scripts/migration-pin.sh` would write for a file holding `sql`.
fn pin_line(sql: &'static str, file: &str) -> String {
    format!("{}  {file}\n", hex(&migration(0, "", sql).checksum))
}

/// Synthetic on purpose: a probe file dropped into `migrations/` would be left behind by an
/// interrupted run, and could be committed.
#[test]
fn a_new_migration_without_a_pin_names_the_command_that_pins_it() {
    let first = migration(1, "settings", "CREATE TABLE a (id INTEGER);\n");
    let added = migration(2, "probe only", "SELECT 1;\n");
    let pins = parse_pins(&pin_line(
        "CREATE TABLE a (id INTEGER);\n",
        "0001_settings.sql",
    ));
    let problems = problems([&first, &added], pins);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert!(
        problems[0].contains("\n    scripts/migration-pin.sh 0002_probe_only.sql"),
        "{}",
        problems[0]
    );
}

#[test]
fn a_changed_comment_fails_and_points_at_a_new_migration() {
    let pins = parse_pins(&pin_line("-- one\nSELECT 1;\n", "0003_views.sql"));
    let edited = migration(3, "views", "-- two\nSELECT 1;\n");
    let problems = problems([&edited], pins);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert!(
        problems[0].contains("migration 3 was previously applied but has been modified")
            && problems[0].contains("new migration"),
        "{}",
        problems[0]
    );
}

#[test]
fn a_removed_migration_fails() {
    let pins = parse_pins(&pin_line("SELECT 1;\n", "0007_gone.sql"));
    let problems = problems(std::iter::empty(), pins);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert!(problems[0].contains("0007_gone.sql is pinned but no longer exists"));
}

/// The pins are the checksums sqlx really stores, not a formula assumed to match it: the pins
/// were written by `sha384sum` over the file bytes, so equality here proves the script and sqlx
/// agree.
#[tokio::test]
async fn the_pins_are_what_sqlx_stores_in_a_real_database() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join(rc_db::DB_FILE);
    let db = rc_db::Db::open(&path).await.expect("open database");
    db.close().await;

    let url = format!("sqlite://{}", path.display());
    let mut connection = SqliteConnection::connect(&url).await.expect("connect");
    let stored: Vec<(i64, Vec<u8>, bool)> =
        sqlx::query_as("SELECT version, checksum, success FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&mut connection)
            .await
            .expect("read _sqlx_migrations");
    connection.close().await.expect("close");

    let pins = parse_pins(PINS);
    assert_eq!(
        stored
            .iter()
            .map(|(version, ..)| *version)
            .collect::<Vec<_>>(),
        pins.keys().copied().collect::<Vec<_>>(),
        "the pinned numbers are not the numbers sqlx applied"
    );
    for (version, checksum, success) in &stored {
        assert!(success, "migration {version} did not apply");
        assert_eq!(
            hex(checksum),
            pins[version].checksum,
            "{}: the pin is not what sqlx stored",
            pins[version].file
        );
    }
}
