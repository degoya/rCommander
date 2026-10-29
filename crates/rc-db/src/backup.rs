//! Backups of the database: one consistent SQLite file written with `VACUUM INTO`, at any time -
//! also while rCommander runs and writes (RC-010-12).
//!
//! `VACUUM INTO` reads the database inside one read transaction, so the copy is the state of one
//! moment, and in WAL mode the writer goes on meanwhile. The copy is written to a temporary file
//! beside the target, has to pass `PRAGMA integrity_check`, is flushed to disk and only then takes
//! the target's name: the target either holds a checked backup or is untouched. An existing
//! target is replaced only on request (`overwrite`); otherwise the name is taken with a hard link,
//! which fails on an existing name, so no file that appeared meanwhile is overwritten either.
//!
//! On Unix the file is created `0600`; on Windows it inherits the ACL of its folder.
//!
//! The copy runs on a task of its own. A caller that stops waiting does not tear SQLite's work
//! apart: the copy finishes, is not given the target's name, and is removed.

mod error;
mod private_file;
mod prune;

use std::{
    fs::{self, OpenOptions},
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};
use sqlx::{Connection, SqliteConnection, SqlitePool, sqlite::SqliteConnectOptions};
use tokio::sync::oneshot;

pub use error::BackupError;
pub use private_file::write_private_file;
pub use prune::{KEEP_RANGE, prune_backups};

use crate::{BUSY_TIMEOUT, Db};

/// The folder for backups beside the database file (`<data dir>/backups`): where MCP's
/// `db_backup` writes, under a generated name. `rc_mcp::BACKUP_DIR` (RC-020-07) names the same
/// folder for the MCP read guard.
pub const BACKUP_DIR: &str = "backups";

/// The generated names: `rcommander-YYYYMMDD-HHMMSS.db`.
const NAME_PREFIX: &str = "rcommander-";
const NAME_EXTENSION: &str = ".db";

/// How many numbered names (`-2`, `-3`, ...) are tried when a generated name is taken.
const NAME_ATTEMPTS: u32 = 100;

/// The files SQLite keeps beside a database while it works on it.
pub(crate) const SIDECARS: [&str; 3] = ["-journal", "-wal", "-shm"];

/// A finished, checked backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupReport {
    /// The backup file.
    pub path: PathBuf,
    /// Its size in bytes.
    pub size: u64,
}

/// The generated file name of a backup taken at `stamp` (`YYYYMMDD-HHMMSS`, local time):
/// `rcommander-<stamp>.db`.
#[must_use]
pub fn backup_file_name(stamp: &str) -> String {
    format!("{NAME_PREFIX}{stamp}{NAME_EXTENSION}")
}

/// Whether `name` is one of rCommander's database files: the database (`rcommander.db` and what
/// SQLite keeps beside it), a staged restore, a backup under a generated name
/// (`rcommander-*.db`) or a backup being written. For the MCP read guard: no agent reads one.
#[must_use]
pub fn is_database_file_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let name = name.strip_prefix('.').unwrap_or(&name);
    name.starts_with("rcommander") && name.contains(NAME_EXTENSION)
}

impl Db {
    /// Backs the database up into `target`: a file of that name, or a file with a generated name
    /// in it when `target` is a folder. Reads through the reader pool, so the writer waits on
    /// nothing it would not wait on anyway.
    ///
    /// # Errors
    ///
    /// The target exists and `overwrite` is false ([`BackupError::TargetExists`]), its folder is
    /// missing or not writable, the copy fails or does not pass its check. Nothing is left at
    /// the target then.
    pub async fn backup_to(
        &self,
        target: impl AsRef<Path>,
        overwrite: bool,
    ) -> Result<BackupReport, BackupError> {
        let source = Source::Pool {
            readers: self.readers.clone(),
            file: self.path().map(Path::to_path_buf),
        };
        run(source, target.as_ref().to_path_buf(), overwrite, "").await
    }

    /// The folder for backups beside the database file ([`BACKUP_DIR`]).
    ///
    /// # Errors
    ///
    /// [`BackupError::InMemory`] for a database in memory.
    pub fn backup_dir(&self) -> Result<PathBuf, BackupError> {
        self.path()
            .and_then(Path::parent)
            .map(|data| data.join(BACKUP_DIR))
            .ok_or(BackupError::InMemory)
    }

    /// Backs the database up into [`Db::backup_dir`] under a generated name, creating the
    /// folder: the only place MCP writes a backup to.
    ///
    /// # Errors
    ///
    /// As [`Db::backup_to`]; [`BackupError::InMemory`] for a database in memory.
    pub async fn backup_to_backup_dir(&self) -> Result<BackupReport, BackupError> {
        self.backup_named("").await
    }

    /// A backup into [`Db::backup_dir`] whose generated name ends in `suffix`.
    pub(crate) async fn backup_named(
        &self,
        suffix: &'static str,
    ) -> Result<BackupReport, BackupError> {
        let folder = self.backup_dir()?;
        fs::create_dir_all(&folder).map_err(|source| BackupError::Target {
            path: folder.clone(),
            source,
        })?;
        let source = Source::Pool {
            readers: self.readers.clone(),
            file: self.path().map(Path::to_path_buf),
        };
        run(source, folder, false, suffix).await
    }
}

/// Backs up the database file `database` into `target`, through a connection of its own: for
/// `rcommander backup`, also while the window has the file open. Runs no migration and changes
/// nothing in the database.
///
/// # Errors
///
/// [`BackupError::NoDatabase`] when there is no file at `database`; otherwise as
/// [`Db::backup_to`].
pub async fn backup_file(
    database: impl AsRef<Path>,
    target: impl AsRef<Path>,
    overwrite: bool,
) -> Result<BackupReport, BackupError> {
    let database = database.as_ref().to_path_buf();
    if !database.is_file() {
        return Err(BackupError::NoDatabase { path: database });
    }
    run(
        Source::File(database),
        target.as_ref().to_path_buf(),
        overwrite,
        "",
    )
    .await
}

/// What a backup reads from.
enum Source {
    /// An open database's readers, and its file.
    Pool {
        readers: SqlitePool,
        file: Option<PathBuf>,
    },
    /// A database file, opened for the backup alone.
    File(PathBuf),
}

/// Runs the backup on a task of its own and waits for it. A caller that stops waiting closes
/// the channel; the task notices before it names the file, and nothing is published.
async fn run(
    source: Source,
    target: PathBuf,
    overwrite: bool,
    suffix: &'static str,
) -> Result<BackupReport, BackupError> {
    let (done, result) = oneshot::channel();
    tokio::spawn(async move {
        let stopped = || done.is_closed();
        let outcome = match source {
            Source::Pool { readers, file } => match readers.acquire().await {
                Ok(mut reader) => {
                    let plan = Plan {
                        target: &target,
                        database: file.as_deref(),
                        overwrite,
                        suffix,
                    };
                    write_verified(&mut reader, &plan, &stopped).await
                }
                Err(error) => Err(error.into()),
            },
            Source::File(file) => {
                let options = SqliteConnectOptions::new()
                    .filename(&file)
                    .busy_timeout(BUSY_TIMEOUT);
                match SqliteConnection::connect_with(&options).await {
                    Ok(mut connection) => {
                        let plan = Plan {
                            target: &target,
                            database: Some(file.as_path()),
                            overwrite,
                            suffix,
                        };
                        let written = write_verified(&mut connection, &plan, &stopped).await;
                        // A failed close loses nothing: the connection only read.
                        let _ = connection.close().await;
                        written
                    }
                    Err(error) => Err(error.into()),
                }
            }
        };
        // Nobody waits any more when this fails; the copy was not published.
        let _ = done.send(outcome);
    });
    result.await.unwrap_or(Err(BackupError::Interrupted))
}

/// Where one backup goes.
struct Plan<'a> {
    /// A file, or a folder for a generated name.
    target: &'a Path,
    /// The database file being backed up, which is never a target.
    database: Option<&'a Path>,
    overwrite: bool,
    /// The end of a generated name, before the extension.
    suffix: &'static str,
}

/// The backup itself: `VACUUM INTO` a temporary file beside the target, the integrity check, a
/// flush to disk, then the target's name - unless `stopped` says the caller left.
async fn write_verified(
    connection: &mut SqliteConnection,
    plan: &Plan<'_>,
    stopped: &(dyn Fn() -> bool + Sync),
) -> Result<BackupReport, BackupError> {
    let (target, overwrite) = resolve(connection, plan).await?;
    if let Some(database) = plan.database
        && same_file(&target, database)
    {
        return Err(BackupError::TargetIsDatabase { path: target });
    }
    if !overwrite && exists(&target) {
        return Err(BackupError::TargetExists { path: target });
    }
    let part = PartFile::create(&target)?;
    let into = part
        .path
        .to_str()
        .ok_or_else(|| BackupError::PathNotUnicode {
            path: target.clone(),
        })?;
    sqlx::query("VACUUM INTO ?")
        .bind(into)
        .persistent(false)
        .execute(&mut *connection)
        .await?;
    verify(&part.path).await?;
    flush(&part.path)
        .await
        .map_err(|source| BackupError::Target {
            path: target.clone(),
            source,
        })?;
    if stopped() {
        return Err(BackupError::Interrupted);
    }
    publish(&part.path, &target, overwrite)?;
    let size = fs::metadata(&target)
        .map_err(|source| BackupError::Target {
            path: target.clone(),
            source,
        })?
        .len();
    Ok(BackupReport { path: target, size })
}

/// The absolute file the backup goes to, and whether it may replace one: a folder gets the
/// first free generated name, which never replaces anything.
async fn resolve(
    connection: &mut SqliteConnection,
    plan: &Plan<'_>,
) -> Result<(PathBuf, bool), BackupError> {
    let absolute = std::path::absolute(plan.target).map_err(|source| BackupError::Target {
        path: plan.target.to_path_buf(),
        source,
    })?;
    if !absolute.is_dir() {
        return Ok((absolute, plan.overwrite));
    }
    let stamp: String = sqlx::query_scalar("SELECT strftime('%Y%m%d-%H%M%S', 'now', 'localtime')")
        .fetch_one(&mut *connection)
        .await?;
    let stem = format!("{NAME_PREFIX}{stamp}{}", plan.suffix);
    let mut candidate = absolute.join(format!("{stem}{NAME_EXTENSION}"));
    for number in 2..=NAME_ATTEMPTS {
        if !exists(&candidate) {
            break;
        }
        candidate = absolute.join(format!("{stem}-{number}{NAME_EXTENSION}"));
    }
    Ok((candidate, false))
}

/// Whether anything has this name; a dangling link counts.
pub(crate) fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Whether two paths name one file; false where either cannot be resolved.
fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// `None` when SQLite finds the database of `connection` sound, the problems it reports
/// otherwise.
pub(crate) async fn integrity_problems(
    connection: &mut SqliteConnection,
) -> Result<Option<String>, sqlx::Error> {
    let rows: Vec<String> = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_all(&mut *connection)
        .await?;
    Ok((rows != ["ok"]).then(|| rows.join("; ")))
}

/// The copy, opened read-only on its own, passes `PRAGMA integrity_check`.
async fn verify(path: &Path) -> Result<(), BackupError> {
    let options = SqliteConnectOptions::new().filename(path).read_only(true);
    let mut connection = SqliteConnection::connect_with(&options).await?;
    let problems = integrity_problems(&mut connection).await;
    // A failed close loses nothing: the connection only read.
    let _ = connection.close().await;
    match problems? {
        None => Ok(()),
        Some(detail) => Err(BackupError::Unverified { detail }),
    }
}

/// Writes the file's data to the disk before it gets its name.
pub(crate) async fn flush(path: &Path) -> io::Result<()> {
    tokio::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .await?
        .sync_all()
        .await
}

/// Gives the finished copy the target's name: replacing the target with `overwrite`, never
/// otherwise.
fn publish(part: &Path, target: &Path, overwrite: bool) -> Result<(), BackupError> {
    let failed = |source| BackupError::Target {
        path: target.to_path_buf(),
        source,
    };
    if overwrite {
        return fs::rename(part, target).map_err(failed);
    }
    match fs::hard_link(part, target) {
        // The temporary name goes when the part file drops.
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            Err(BackupError::TargetExists {
                path: target.to_path_buf(),
            })
        }
        // A file system without hard links (FAT on a USB stick): the name is looked at once
        // more and taken by a rename.
        Err(_) if exists(target) => Err(BackupError::TargetExists {
            path: target.to_path_buf(),
        }),
        Err(_) => fs::rename(part, target).map_err(failed),
    }
}

/// A file being written beside its target under a temporary name (`.<name>.<pid>-<n>.part`).
/// Dropping it removes the temporary name and SQLite's files beside it; a published copy has its
/// own name by then.
pub(crate) struct PartFile {
    pub(crate) path: PathBuf,
}

impl PartFile {
    /// Creates the empty file - owner-only on Unix - so the name is reserved; `VACUUM INTO`
    /// accepts an empty file.
    pub(crate) fn create(target: &Path) -> Result<Self, BackupError> {
        Self::create_open(target).map(|(part, _)| part)
    }

    /// [`PartFile::create`] with the file it opened: `create_new`, so an existing file or link
    /// at the name is refused, never followed.
    pub(crate) fn create_open(target: &Path) -> Result<(Self, fs::File), BackupError> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let failed = |source| BackupError::Target {
            path: target.to_path_buf(),
            source,
        };
        let (Some(folder), Some(name)) = (target.parent(), target.file_name()) else {
            return Err(failed(io::Error::from(io::ErrorKind::InvalidInput)));
        };
        let path = folder.join(format!(
            ".{}.{}-{}.part",
            name.to_string_lossy(),
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        owner_only(&mut options);
        let file = options.open(&path).map_err(failed)?;
        Ok((Self { path }, file))
    }
}

impl Drop for PartFile {
    fn drop(&mut self) {
        // Missing files are the normal case; anything else leaves a recognisable `.part`.
        let _ = fs::remove_file(&self.path);
        for sidecar in SIDECARS {
            let mut name = self.path.clone().into_os_string();
            name.push(sidecar);
            let _ = fs::remove_file(PathBuf::from(name));
        }
    }
}

#[cfg(unix)]
fn owner_only(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt as _;
    options.mode(0o600);
}

/// Windows: the file inherits the ACL of its folder.
#[cfg(not(unix))]
fn owner_only(_options: &mut OpenOptions) {}

#[cfg(test)]
#[path = "backup_tests.rs"]
mod tests;
