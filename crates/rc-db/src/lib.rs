//! rCommander's SQLite database (`rcommander.db`): the migrations, the settings, the connection
//! tree and each connection's SSH session settings (`session_prefs.rs`), the SSH host keys a
//! person trusted (`known_hosts`), the error log, the multi-rename undo log (`rename_log`), the
//! vault's metadata and its encrypted columns (`secrets.rs`), the login scripts
//! (`login_scripts.rs`), the panels' folder tabs with their history (`tabs.rs`), the directory
//! hotlist (`hotlist.rs`), the FTPS certificates a person trusted (`tls_certificates.rs`), the
//! transfer jobs that have not ended, to resume after a restart (`transfer_jobs.rs`), the
//! transfer lists a person saved (`saved_queues.rs`) and the own colour themes (`themes.rs`).
//!
//! Writes are serialized: one dedicated connection behind a mutex does every write, and a small
//! pool serves reads. SQLite allows one writer at a time anyway; taking turns in the process
//! instead of in the file means a write waits on the mutex rather than failing with
//! `SQLITE_BUSY`, and a multi-statement write later holds the lock for its whole transaction.
//! `busy_timeout` still covers a second process on the same file (the GUI and a headless
//! `rcommander mcp`). In WAL mode readers never wait for the writer.
//!
//! Queries are runtime queries (`sqlx::query`), never the `query!` macros. Migrations live in
//! `migrations/` and never change once applied: `migrations.sha384` pins every file, and
//! `tests/migration_checksums.rs` fails when one changes or arrives without a pin
//! (`scripts/migration-pin.sh`).
//!
//! The file holds durable state only (RC-010-11). A table that grows with use is listed in
//! [`RETAINED_TABLES`] and kept within a row and an age limit by [`Db::prune`]; every other
//! table is in [`DURABLE_TABLES`] with its reason. The file is opened with
//! `auto_vacuum = INCREMENTAL`, so what the sweep deletes goes back to the file system.
//!
//! Every connection runs with `secure_delete`: what a write replaces or deletes is overwritten
//! with zeros in the file, not only unlinked - above all the ciphertexts, salt and verifier a
//! change of the master password replaces (RC-080-11), which with the old password would still
//! open.
//!
//! A backup ([`Db::backup_to`], [`backup_file`]) is one consistent SQLite file written with
//! `VACUUM INTO` while the program runs; a restore ([`Db::restore_from`]) is checked, staged
//! beside the database and swapped in before the next open ([`apply_pending_restore`]).

mod backup;
pub mod connections;
mod diagnostics;
mod error;
mod error_log;
mod hotlist;
mod known_hosts;
pub mod login_scripts;
mod plugins;
mod private_dir;
pub mod redact;
mod rename_log;
mod restore;
mod retention;
mod saved_queues;
mod secret_sources;
mod secrets;
mod session_prefs;
mod settings;
mod tabs;
mod themes;
mod tls_certificates;
mod transfer_jobs;
mod vault;

use std::{
    path::Path,
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex as SyncMutex},
    time::Duration,
};

use sqlx::{
    Connection, SqliteConnection, SqlitePool,
    migrate::Migrator,
    sqlite::{
        SqliteAutoVacuum, SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions,
        SqliteSynchronous,
    },
};
use tokio::sync::{Mutex, MutexGuard};

pub use backup::{
    BACKUP_DIR, BackupError, BackupReport, KEEP_RANGE as BACKUP_KEEP_RANGE, backup_file,
    backup_file_name, is_database_file_name, prune_backups, write_private_file,
};
pub use diagnostics::{SWEEP_INTERVAL, install_diagnostics, install_diagnostics_with};
pub use error::{DbError, Result};
pub use error_log::{ERROR_LOG_MAX_PAGE, ERROR_LOG_PAGE, ErrorLogEntry, ErrorLogQuery, LogLevel};
pub use hotlist::{
    HOTLIST_MAX_DEPTH, HOTLIST_MAX_NAME, HotlistError, HotlistItem, HotlistPatch, NewHotlistItem,
};
pub use known_hosts::{HASHED_PREFIX, KnownHost, KnownHostSource, KnownHostsImport, NewKnownHost};
pub use plugins::{
    NewPluginExecution, PLUGIN_EXECUTIONS_MAX_PAGE, PluginExecution, StoredPlugin, StoredPluginKey,
};
pub use rename_log::{LoggedStep, RenameRun};
pub use restore::{
    BackupInfo, RESTORE_DIGEST_KEY, RestoreReport, apply_pending_restore, is_rcommander_database,
    looks_like_rcommander_database, rejected_restore_path, staged_restore_path,
};
pub use retention::{
    CheckedLimits, DURABLE_TABLES, PRUNE_BATCH, PruneReport, RETAINED_TABLES, RetainedTable,
    RetentionError, RetentionLimits, RetentionSettings, RetentionStatus, retained_table,
};
pub use saved_queues::SavedQueue;
pub use secret_sources::{MAX_SOURCE_ENTRY_ID_CHARS, MAX_SOURCE_LABEL_CHARS, SecretSourceRef};
pub use secrets::{
    CREDENTIALS_TABLE, SECRETS_ROW_ID, SECRETS_TABLE, SecretFlags, SecretKind, SecretOwner,
    StoredKey,
};
pub use session_prefs::StoredSessionPrefs;
pub use tabs::{StoredTab, StoredVisit};
pub use themes::StoredTheme;
pub use tls_certificates::{NewTrustedCertificate, TrustedCertificate};
pub use transfer_jobs::StoredTransfer;
pub use vault::{RekeyError, SECRET_COLUMNS, SecretColumn, VaultMeta, VaultRemoval};

/// The database file name inside the data directory.
pub const DB_FILE: &str = "rcommander.db";

/// How long a statement waits for another process's lock before it fails.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Connections that serve reads.
const READERS: u32 = 4;

/// `PRAGMA auto_vacuum` of a file that gives freed pages back on `incremental_vacuum`.
const AUTO_VACUUM_INCREMENTAL: i64 = 2;

/// The migrations this build carries, compiled in from `migrations/`.
static MIGRATOR: Migrator = sqlx::migrate!();

/// The database: a reader pool and the one writer connection. Cheap to clone; clones share both.
#[derive(Clone)]
pub struct Db {
    readers: SqlitePool,
    writer: Arc<Mutex<SqliteConnection>>,
    /// The file; `None` in memory.
    path: Option<Arc<Path>>,
    /// The salt of the vault key this process holds, while it holds one: every write of a
    /// secret column checks it against `vault_meta` in its transaction (`vault.rs`).
    vault_key: Arc<SyncMutex<Option<Vec<u8>>>>,
}

impl Db {
    /// Opens (or creates) the database file at `path`, creating its directory - owner-only on
    /// Unix, and the platform's data folder made so when it is there already (RC-080-42) - and
    /// applies every pending migration.
    ///
    /// # Errors
    ///
    /// The directory cannot be created, the file cannot be opened, or a migration fails.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            let data_dir = Self::default_path().ok();
            let data_dir = data_dir.as_deref().and_then(Path::parent);
            private_dir::create_private_dir(parent, data_dir)
                .await
                .map_err(|source| DbError::CreateDir {
                    path: parent.to_path_buf(),
                    source,
                })?;
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal);
        Self::connect(options, Some(path.into())).await
    }

    /// A private database in memory, migrated like a file: for tests and a headless run that
    /// must not touch the user's data.
    ///
    /// # Errors
    ///
    /// A migration fails.
    pub async fn open_in_memory() -> Result<Self> {
        // Every `:memory:` parse names a new shared-cache database, so the writer and the
        // readers see the same one; it lives as long as the writer connection.
        Self::connect(SqliteConnectOptions::from_str("sqlite::memory:")?, None).await
    }

    /// `rcommander.db` in the platform's data directory (on Windows
    /// `%APPDATA%\degoya\rCommander\data`).
    ///
    /// # Errors
    ///
    /// [`DbError::NoDataDir`] when the platform reports no home directory.
    pub fn default_path() -> Result<PathBuf> {
        directories::ProjectDirs::from("de", "degoya", "rCommander")
            .map(|dirs| dirs.data_dir().join(DB_FILE))
            .ok_or(DbError::NoDataDir)
    }

    /// The database file; `None` for a database in memory.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    async fn connect(options: SqliteConnectOptions, path: Option<Arc<Path>>) -> Result<Self> {
        // `auto_vacuum` decides the layout of a new file; on an existing one it only names the
        // mode the next `VACUUM` switches to.
        let options = options
            .foreign_keys(true)
            .busy_timeout(BUSY_TIMEOUT)
            .auto_vacuum(SqliteAutoVacuum::Incremental)
            .pragma("secure_delete", "ON");
        let mut writer = SqliteConnection::connect_with(&options).await?;
        switch_to_incremental_vacuum(&mut writer).await;
        MIGRATOR.run(&mut writer).await?;
        let readers = SqlitePoolOptions::new()
            .max_connections(READERS)
            .min_connections(1)
            .connect_with(options)
            .await?;
        Ok(Self {
            readers,
            writer: Arc::new(Mutex::new(writer)),
            path,
            vault_key: Arc::new(SyncMutex::new(None)),
        })
    }

    /// The pool for reads. Never write through it: writes go through [`Db::writer`].
    pub(crate) fn readers(&self) -> &SqlitePool {
        &self.readers
    }

    /// The writer connection, held exclusively until the guard drops. A multi-statement write
    /// begins its transaction on it (`BEGIN IMMEDIATE`) and commits before letting go.
    pub(crate) async fn writer(&self) -> MutexGuard<'_, SqliteConnection> {
        self.writer.lock().await
    }

    /// Closes the readers and, when this is the last clone, the writer; WAL is checkpointed
    /// on that last close.
    pub async fn close(self) {
        self.readers.close().await;
        if let Ok(writer) = Arc::try_unwrap(self.writer) {
            let _ = writer.into_inner().close().await;
        }
    }
}

/// Switches a file created without `auto_vacuum` to the incremental mode, once: `VACUUM` rewrites
/// it in the mode the connection asked for. Whether it did. A failure is reported and the file
/// opens as it is - it only keeps its freed pages, as before.
async fn switch_to_incremental_vacuum(writer: &mut SqliteConnection) -> bool {
    let mode: i64 = match sqlx::query_scalar("PRAGMA auto_vacuum")
        .fetch_one(&mut *writer)
        .await
    {
        Ok(mode) => mode,
        Err(error) => {
            tracing::warn!(%error, "the database's auto_vacuum mode cannot be read");
            return false;
        }
    };
    if mode == AUTO_VACUUM_INCREMENTAL {
        return false;
    }
    match sqlx::query("VACUUM").execute(&mut *writer).await {
        Ok(_) => true,
        Err(error) => {
            tracing::warn!(%error, "the database cannot be switched to incremental vacuum");
            false
        }
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
