//! The errors of the database.

use std::{io, path::PathBuf};

/// The result of every database call.
pub type Result<T, E = DbError> = std::result::Result<T, E>;

/// What went wrong with the database.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DbError {
    /// The platform reports no data directory (no home directory).
    #[error("no data directory for rCommander on this system")]
    NoDataDir,
    /// The database's directory could not be created.
    #[error("create database directory {}: {source}", path.display())]
    CreateDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// SQLite or the driver failed.
    #[error("database: {0}")]
    Sqlx(#[from] sqlx::Error),
    /// A migration failed or the applied ones do not match this build.
    #[error("database migration: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    /// A setting's value could not be encoded, or the stored JSON does not fit the type asked.
    #[error("setting {key}: {source}")]
    Json {
        key: String,
        #[source]
        source: serde_json::Error,
    },
    /// A table or column name that is not a plain SQL identifier.
    #[error("not a plain SQL identifier: {name:?}")]
    BadIdentifier { name: String },
    /// The vault's row is missing where a vault was expected.
    #[error("no vault exists in this database")]
    NoVault,
    /// A secret write under a key whose salt is no longer the vault's: another process changed
    /// the master password (or removed and re-created the vault) since this one unlocked.
    #[error("the vault's master password was changed since this key was derived")]
    VaultChanged,
}
