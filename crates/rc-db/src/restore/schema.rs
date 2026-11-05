//! The schema a restored file must have (RC-080-68): exactly the objects the migrations up to its
//! last one create. Tables and indexes are compared by type, name and table; triggers and views
//! by their SQL too, since they run code on every later write or read. A backup with a trigger,
//! a view or a table of its own - or one whose trigger was changed - is refused; one that lacks
//! an object is not this build's schema either. SQLite's own objects (`sqlite_*`: automatic
//! indexes, `sqlite_sequence`, the statistics of `ANALYZE`) are SQLite's to make and left out.

use std::collections::BTreeSet;

use sqlx::{
    SqliteConnection,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

use crate::MIGRATOR;

/// One object of `sqlite_schema` as compared: type, name, its table, and the SQL of a trigger
/// or a view.
type Object = (String, String, String, Option<String>);

/// The objects compared, SQLite's own left out (a name with the `sqlite_` prefix is reserved to
/// SQLite: `CREATE` refuses it).
const OBJECTS: &str = "SELECT type, name, tbl_name, \
     CASE WHEN type IN ('trigger', 'view') THEN sql END \
     FROM sqlite_schema WHERE name NOT LIKE 'sqlite\\_%' ESCAPE '\\'";

/// How a file's schema differs from the one its migrations create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Difference {
    /// An object the migrations do not create, or one changed: its type and name.
    Foreign { kind: String, name: String },
    /// An object the migrations create is missing.
    Missing,
}

/// The objects of the database behind `connection`.
async fn objects(connection: &mut SqliteConnection) -> Result<BTreeSet<Object>, sqlx::Error> {
    let rows: Vec<Object> = sqlx::query_as(OBJECTS).fetch_all(connection).await?;
    Ok(rows.into_iter().collect())
}

/// The objects the migrations up to `version` create, replayed in a database in memory.
///
/// Through a pool of one connection, not a borrowed `SqliteConnection`: `Migrator::run_to` over
/// `&mut SqliteConnection` fails the higher-ranked `Send` check of the futures that await this
/// (the engine's dispatch: "implementation of `Acquire` is not general enough").
async fn expected(version: i64) -> Result<BTreeSet<Object>, sqlx::Error> {
    let options = SqliteConnectOptions::new().in_memory(true);
    let memory = SqlitePoolOptions::new()
        .max_connections(1)
        .min_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect_with(options)
        .await?;
    let replayed = MIGRATOR
        .run_to(version, &memory)
        .await
        .map_err(|error| sqlx::Error::Migrate(Box::new(error)));
    let found = match replayed {
        Ok(()) => sqlx::query_as::<_, Object>(OBJECTS)
            .fetch_all(&memory)
            .await
            .map(|rows| rows.into_iter().collect()),
        Err(error) => Err(error),
    };
    // The database lived in memory, in the pool's one connection: closing it loses nothing.
    memory.close().await;
    found
}

/// How the schema behind `connection` differs from the one the migrations up to `version`
/// create - the first foreign object by type and name - or `None` when it is the same.
///
/// # Errors
///
/// A query on either database fails, or the migrations do not replay.
pub(super) async fn difference(
    connection: &mut SqliteConnection,
    version: i64,
) -> Result<Option<Difference>, sqlx::Error> {
    let found = objects(connection).await?;
    let wanted = expected(version).await?;
    if let Some((kind, name, _, _)) = found.difference(&wanted).next() {
        return Ok(Some(Difference::Foreign {
            kind: kind.clone(),
            name: shown_name(name),
        }));
    }
    if wanted.difference(&found).next().is_some() {
        return Ok(Some(Difference::Missing));
    }
    Ok(None)
}

/// The most characters of a foreign object's name an error carries.
const MAX_SHOWN_NAME: usize = 64;

/// `name` for an error: the file's, so no longer than [`MAX_SHOWN_NAME`] characters and without
/// control characters.
fn shown_name(name: &str) -> String {
    name.chars()
        .filter(|character| !character.is_control())
        .take(MAX_SHOWN_NAME)
        .collect()
}

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
