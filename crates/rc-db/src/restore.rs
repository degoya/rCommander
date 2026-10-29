//! Restoring a backup (RC-010-12): checked, staged beside the database, swapped in before the next
//! open.
//!
//! [`Db::restore_from`] copies the chosen file next to the database under a temporary name and
//! checks the copy - the bytes that will be swapped in: `PRAGMA integrity_check`, rCommander's
//! tables, and migrations this build knows with the same checksums (a newer file is refused,
//! since this build cannot open it). Then it backs the current state up into the backup folder,
//! stores the copy's BLAKE3 digest inside the current database (the setting
//! [`RESTORE_DIGEST_KEY`]) and only then names the copy `<database>.restore`.
//! [`apply_pending_restore`], called before the database is opened at the next start, reads that
//! digest from the current database and swaps in only a staged file with exactly that digest
//! (compared in constant time) that passes the same check again (RC-080-09). A write tool can
//! put a file beside the database, never a row into it: a file of that name someone else put
//! there - an agent, another program - is set aside as `<database>.restore-rejected` and the
//! current database stays. Then the old WAL files go and the staged file is renamed over the
//! database; the migrations run as on every start.

use std::{
    fs,
    io::{self, Read as _},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use tokio::io::AsyncReadExt as _;

use crate::{
    Db, MIGRATOR,
    backup::{BackupError, BackupReport, PartFile, SIDECARS, flush, integrity_problems},
};

/// The end of the safety backup's generated name: `rcommander-<stamp>-before-restore.db`.
const SAFETY_SUFFIX: &str = "-before-restore";

/// The first bytes of every SQLite database file.
const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

/// The name of the vault's table, which only an rCommander database has.
const VAULT_TABLE: &[u8] = b"vault_meta";

/// The setting the window's restore keeps the staged file's BLAKE3 digest under (hex), inside
/// the database it replaces: no schema key, so `settings.set` refuses it, and no MCP tool writes
/// the database file.
pub const RESTORE_DIGEST_KEY: &str = "db.restore_digest";

/// What the check of a backup found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupInfo {
    /// The file that was chosen.
    pub path: PathBuf,
    /// Its size in bytes.
    pub size: u64,
    /// The last migration applied to it.
    pub schema_version: i64,
    /// Whether it holds a vault: its secrets need the master password they were stored with.
    pub has_vault: bool,
}

/// A restore, staged for the next start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreReport {
    /// The checked backup.
    pub checked: BackupInfo,
    /// The backup of the state it replaces.
    pub safety_backup: BackupReport,
    /// The staged file, swapped in at the next start.
    pub staged: PathBuf,
}

/// Where a restore waits for the next start: `<database>.restore`.
#[must_use]
pub fn staged_restore_path(database: &Path) -> PathBuf {
    beside(database, ".restore")
}

/// Where a staged file that failed its check at the start is set aside:
/// `<database>.restore-rejected`, replacing one an earlier start set aside.
#[must_use]
pub fn rejected_restore_path(database: &Path) -> PathBuf {
    beside(database, ".restore-rejected")
}

/// `path` with `suffix` appended to its name.
fn beside(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

impl Db {
    /// Checks the backup at `source` and stages it to replace this database at the next start,
    /// after backing the current state up into [`Db::backup_dir`]. Nothing changes when the check
    /// or the safety backup fails.
    ///
    /// # Errors
    ///
    /// [`BackupError::RestoreNotDatabase`], [`BackupError::RestoreForeign`] or
    /// [`BackupError::RestoreNewer`] for a file that cannot be restored; [`BackupError::InMemory`]
    /// for a database in memory; a failed safety backup or staging.
    pub async fn restore_from(
        &self,
        source: impl AsRef<Path>,
    ) -> Result<RestoreReport, BackupError> {
        let source = source.as_ref();
        let database = self.path().ok_or(BackupError::InMemory)?;
        let staged = staged_restore_path(database);
        let part = PartFile::create(&staged)?;
        tokio::fs::copy(source, &part.path).await.map_err(|error| {
            BackupError::RestoreNotDatabase {
                path: source.to_path_buf(),
                detail: error.to_string(),
            }
        })?;
        let checked = check(&part.path, source).await?;
        let failed = |error| BackupError::Target {
            path: staged.clone(),
            source: error,
        };
        flush(&part.path).await.map_err(failed)?;
        let digest = file_digest(&part.path).await.map_err(failed)?;
        let safety_backup = self.backup_named(SAFETY_SUFFIX).await?;
        // The digest first: a staged file never waits without it.
        self.set(RESTORE_DIGEST_KEY, digest.to_hex().as_str())
            .await
            .map_err(|error| failed(io::Error::other(error)))?;
        fs::rename(&part.path, &staged).map_err(failed)?;
        Ok(RestoreReport {
            checked,
            safety_backup,
            staged,
        })
    }
}

/// Swaps a staged restore in for the database at `database`, before anything opens it. Answers
/// whether there was one. Only a file is taken: a folder or a link under the name is nobody's
/// restore and stays as it is. The file must have the digest [`Db::restore_from`] stored in the
/// current database and pass that check again; otherwise it is set aside
/// ([`rejected_restore_path`]) and the database is unchanged. The digest is cleared either way.
/// The old WAL files go before the swap: SQLite would otherwise replay them into the restored
/// file. The state they held is in the safety backup.
///
/// # Errors
///
/// [`BackupError::RestoreRejected`] when the staged file was set aside;
/// [`BackupError::RestoreSwap`] when a file cannot be removed or renamed - on Windows while
/// another program has the database open. The staged file then waits for the next start.
pub async fn apply_pending_restore(database: &Path) -> Result<bool, BackupError> {
    let staged = staged_restore_path(database);
    match fs::symlink_metadata(&staged) {
        Ok(meta) if meta.file_type().is_file() => {}
        _ => return Ok(false),
    }
    let swap = |source| BackupError::RestoreSwap { source };
    let expected = stored_digest(database).await;
    if let Err(reason) = confirm(&staged, expected).await {
        let set_aside = rejected_restore_path(database);
        fs::rename(&staged, &set_aside).map_err(swap)?;
        clear_digest(database).await;
        return Err(BackupError::RestoreRejected { set_aside, reason });
    }
    remove_sidecars(database, &["-wal", "-shm"]).map_err(swap)?;
    fs::rename(&staged, database).map_err(swap)?;
    // A backup taken while a restore waited holds that digest too: none outlives its swap.
    clear_digest(database).await;
    Ok(true)
}

/// The digest the window's restore stored in the database at `database`, read through a
/// connection of its own (its WAL included); `None` when there is none or it cannot be read.
async fn stored_digest(database: &Path) -> Option<blake3::Hash> {
    let options = SqliteConnectOptions::new().filename(database);
    let mut connection = SqliteConnection::connect_with(&options).await.ok()?;
    let stored: Result<Option<String>, sqlx::Error> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(RESTORE_DIGEST_KEY)
            .fetch_optional(&mut connection)
            .await;
    // A failed close loses nothing: the connection only read.
    let _ = connection.close().await;
    let text: String = serde_json::from_str(&stored.ok()??).ok()?;
    blake3::Hash::from_hex(text).ok()
}

/// Removes the digest once its file was swapped in or set aside, so no later file can match it.
async fn clear_digest(database: &Path) {
    let options = SqliteConnectOptions::new().filename(database);
    let Ok(mut connection) = SqliteConnection::connect_with(&options).await else {
        return;
    };
    // A digest left behind matches only the backup the person chose: nothing to report.
    let _ = sqlx::query("DELETE FROM settings WHERE key = ?")
        .bind(RESTORE_DIGEST_KEY)
        .execute(&mut connection)
        .await;
    let _ = connection.close().await;
}

/// Why the staged file must not replace the database, or nothing when it may: it has the digest
/// the window's restore stored (`expected`) and passes the check [`Db::restore_from`] made.
async fn confirm(staged: &Path, expected: Option<blake3::Hash>) -> Result<(), String> {
    let expected =
        expected.ok_or_else(|| "rCommander's restore did not prepare this file".to_owned())?;
    let actual = file_digest(staged)
        .await
        .map_err(|error| error.to_string())?;
    // `blake3::Hash` compares in constant time.
    if actual != expected {
        return Err("its contents differ from what rCommander's restore checked".to_owned());
    }
    // A journal or WAL beside it is not the window's: SQLite would replay it into the file.
    remove_sidecars(staged, &SIDECARS).map_err(|error| error.to_string())?;
    let checked = check(staged, staged).await.map(drop);
    remove_quietly_sidecars(staged);
    checked.map_err(|error| error.to_string())
}

/// Removes SQLite's files with `suffixes` beside `path`; a missing one is fine.
fn remove_sidecars(path: &Path, suffixes: &[&str]) -> io::Result<()> {
    for suffix in suffixes {
        match fs::remove_file(beside(path, suffix)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// What the check left beside the staged file, removed where possible.
fn remove_quietly_sidecars(path: &Path) {
    for suffix in SIDECARS {
        remove_quietly(&beside(path, suffix));
    }
}

/// Removes a file whose removal nothing depends on.
fn remove_quietly(path: &Path) {
    // Missing is the normal case; a file left over is ignored by every later start.
    let _ = fs::remove_file(path);
}

/// The BLAKE3 digest of the file at `path`.
async fn file_digest(path: &Path) -> io::Result<blake3::Hash> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize())
}

/// Whether the file at `path` is an rCommander database: a SQLite file with the vault's table.
/// For the MCP read guard, so it fails closed: a SQLite file that cannot be read to tell counts as
/// one. Only a file with SQLite's header is opened, read-only and without touching its WAL.
pub async fn is_rcommander_database(path: &Path) -> bool {
    let mut header = [0_u8; 16];
    let read = match tokio::fs::File::open(path).await {
        Ok(mut file) => file.read_exact(&mut header).await.is_ok(),
        Err(_) => false,
    };
    if !read || &header != SQLITE_HEADER {
        return false;
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .immutable(true);
    let Ok(mut connection) = SqliteConnection::connect_with(&options).await else {
        return true;
    };
    let found: Result<bool, sqlx::Error> = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'vault_meta')",
    )
    .fetch_one(&mut connection)
    .await;
    // A failed close loses nothing: the connection only read.
    let _ = connection.close().await;
    found.unwrap_or(true)
}

/// [`is_rcommander_database`] without SQLite, for a guard that cannot wait (the engine's read
/// guard, RC-080-09): a regular file with SQLite's header that holds the vault table's name
/// anywhere - where SQLite keeps the schema. Fails closed too: a SQLite file that cannot be read
/// to its end counts as one. Reads only files with the header, in pieces.
#[must_use]
pub fn looks_like_rcommander_database(path: &Path) -> bool {
    // Never open what is no regular file: a pipe would block the guard.
    if !fs::metadata(path).is_ok_and(|meta| meta.is_file()) {
        return false;
    }
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut header = [0_u8; 16];
    if file.read_exact(&mut header).is_err() || &header != SQLITE_HEADER {
        return false;
    }
    let mut buffer = vec![0_u8; 64 * 1024];
    // The end of the last piece, so a name across two pieces is found.
    let keep = VAULT_TABLE.len() - 1;
    let mut filled = 0;
    loop {
        let read = match file.read(&mut buffer[filled..]) {
            Ok(0) => return false,
            Ok(read) => read,
            Err(_) => return true,
        };
        let end = filled + read;
        if buffer[..end]
            .windows(VAULT_TABLE.len())
            .any(|window| window == VAULT_TABLE)
        {
            return true;
        }
        let tail = end.saturating_sub(keep);
        buffer.copy_within(tail..end, 0);
        filled = end - tail;
    }
}

/// Checks the copy at `file` of the backup `shown` (the name the errors carry).
async fn check(file: &Path, shown: &Path) -> Result<BackupInfo, BackupError> {
    let not_database = |detail: String| BackupError::RestoreNotDatabase {
        path: shown.to_path_buf(),
        detail,
    };
    // Read-write on the private copy: a copy of a file in WAL mode needs its `-shm`.
    let options = SqliteConnectOptions::new().filename(file);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .map_err(|error| not_database(error.to_string()))?;
    let inspected = inspect(&mut connection, shown).await;
    // A failed close loses nothing: the check only read.
    let _ = connection.close().await;
    let (schema_version, has_vault) = inspected?;
    let size = fs::metadata(file)
        .map_err(|error| not_database(error.to_string()))?
        .len();
    Ok(BackupInfo {
        path: shown.to_path_buf(),
        size,
        schema_version,
        has_vault,
    })
}

/// The last migration and whether a vault exists, or why the database cannot be restored.
async fn inspect(
    connection: &mut SqliteConnection,
    shown: &Path,
) -> Result<(i64, bool), BackupError> {
    let not_database = |error: sqlx::Error| BackupError::RestoreNotDatabase {
        path: shown.to_path_buf(),
        detail: error.to_string(),
    };
    let foreign = || BackupError::RestoreForeign {
        path: shown.to_path_buf(),
    };
    if let Some(detail) = integrity_problems(connection).await.map_err(not_database)? {
        return Err(BackupError::RestoreNotDatabase {
            path: shown.to_path_buf(),
            detail,
        });
    }
    let tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' \
         AND name IN ('_sqlx_migrations', 'settings', 'vault_meta')",
    )
    .fetch_one(&mut *connection)
    .await
    .map_err(not_database)?;
    if tables != 3 {
        return Err(foreign());
    }
    let applied: Vec<(i64, Vec<u8>, bool)> =
        sqlx::query_as("SELECT version, checksum, success FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&mut *connection)
            .await
            .map_err(|_| foreign())?;
    let known = MIGRATOR
        .iter()
        .filter(|migration| migration.migration_type.is_up_migration())
        .map(|migration| migration.version)
        .max()
        .unwrap_or(0);
    let Some(&(last, _, _)) = applied.last() else {
        return Err(foreign());
    };
    if last > known {
        return Err(BackupError::RestoreNewer {
            path: shown.to_path_buf(),
            found: last,
            known,
        });
    }
    let ours = |version: i64, checksum: &[u8]| {
        MIGRATOR.iter().any(|migration| {
            migration.migration_type.is_up_migration()
                && migration.version == version
                && migration.checksum.as_ref() == checksum
        })
    };
    if !applied
        .iter()
        .all(|(version, checksum, success)| *success && ours(*version, checksum.as_slice()))
    {
        return Err(foreign());
    }
    let has_vault: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM vault_meta)")
        .fetch_one(&mut *connection)
        .await
        .map_err(not_database)?;
    Ok((last, has_vault))
}

#[cfg(test)]
#[path = "restore_tests.rs"]
mod tests;
