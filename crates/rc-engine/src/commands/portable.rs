//! `app.make_portable` (RC-100-95): this copy keeps its data beside the program from the next
//! start on. The folder and the installation are checked first (`rc_update::portable_mode`);
//! then a checked copy of the database goes to `<program>/data/rcommander.db` the way of
//! `db.backup_to`, the installed plugins follow into `data/plugins/`, and the marker
//! `rcommander.portable` is written last - a copy stopped before it changes nothing at the next
//! start. A database already in `data/` is never overwritten. `GuiOnly`: it carries the vault's
//! ciphertext and the connections in plain text onto what may be a stranger's drive.

use std::{
    io,
    path::{Path, PathBuf},
};

use rc_db::{BackupError, Db};
use rc_update::{install_kind, portable_mode};

use crate::{Engine, EngineError, Outcome, UpdateRefusal, recover::LogPanic as _};

impl Engine {
    /// `app.make_portable`: the database's copy report, `data/rcommander.db` beside the program.
    pub(crate) async fn make_portable(&self) -> Result<Outcome, EngineError> {
        if rc_db::is_portable() {
            return Err(refused(
                "update.portable_already",
                "this copy keeps its data beside the program already",
            ));
        }
        let not_found = |detail: String| refused("update.portable_not_writable", detail);
        let executable = std::env::current_exe().map_err(|error| not_found(error.to_string()))?;
        let installation = tokio::task::spawn_blocking(move || install_kind::detect(&executable))
            .await
            .log_panic("engine.make_portable")
            .map_err(|error| not_found(error.to_string()))?;
        let data = portable_mode::check(&installation)
            .map_err(|code| refused(code, "this copy cannot keep its data beside the program"))?;
        let plugins = self
            .plugins_data_dir()
            .map(rc_plugin_host::installed::plugins_root);
        let report = make_portable_at(
            self.db()?,
            &installation.install_dir,
            &data,
            plugins.as_deref(),
        )
        .await?;
        Ok(Outcome::DatabaseBackup(report))
    }
}

/// The copy into `data` and the marker beside the program in `program`, the installed plugins of
/// `plugins` with them. What it wrote goes again when a step fails, and the marker is the last
/// step.
pub(crate) async fn make_portable_at(
    db: &Db,
    program: &Path,
    data: &Path,
    plugins: Option<&Path>,
) -> Result<rc_db::BackupReport, EngineError> {
    let database = data.join(rc_db::DB_FILE);
    if tokio::fs::symlink_metadata(&database).await.is_ok() {
        return Err(refused(
            "update.portable_database_exists",
            format!(
                "{} exists already; it is never overwritten",
                database.display()
            ),
        ));
    }
    let made_data = tokio::fs::symlink_metadata(data).await.is_err();
    let plugins_copy = data.join(rc_plugin_host::installed::PLUGINS_DIR);
    let made_plugins = tokio::fs::symlink_metadata(&plugins_copy).await.is_err();
    let written = write_portable(db, program, data, &database, plugins, &plugins_copy).await;
    if written.is_err() {
        // Nothing half done stays: the next try finds the folder as it was.
        if made_data {
            let _ = tokio::fs::remove_dir_all(data).await;
        } else {
            let _ = tokio::fs::remove_file(&database).await;
            if made_plugins {
                let _ = tokio::fs::remove_dir_all(&plugins_copy).await;
            }
        }
    }
    written
}

async fn write_portable(
    db: &Db,
    program: &Path,
    data: &Path,
    database: &Path,
    plugins: Option<&Path>,
    plugins_copy: &Path,
) -> Result<rc_db::BackupReport, EngineError> {
    tokio::fs::create_dir_all(data)
        .await
        .map_err(|source| unwritable(data, source))?;
    let report = db.backup_to(database, false).await?;
    if let Some(plugins) = plugins.filter(|plugins| plugins.is_dir()) {
        let (from, to) = (plugins.to_path_buf(), plugins_copy.to_path_buf());
        tokio::task::spawn_blocking(move || copy_tree(&from, &to).map_err(|error| (to, error)))
            .await
            .log_panic("engine.make_portable_plugins")
            .map_err(|_| unwritable(plugins_copy, io::Error::other("the copy stopped")))?
            .map_err(|(path, source)| unwritable(&path, source))?;
    }
    let marker = program.join(portable_mode::MARKER);
    tokio::fs::write(&marker, portable_mode::MARKER_TEXT)
        .await
        .map_err(|source| unwritable(&marker, source))?;
    Ok(report)
}

/// Copies the folder `from` to `to`, files and folders; a link is not followed.
fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target: PathBuf = to.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn refused(code: &'static str, detail: impl Into<String>) -> EngineError {
    EngineError::Update(UpdateRefusal::new(code, detail))
}

fn unwritable(path: &Path, source: io::Error) -> EngineError {
    EngineError::Backup(BackupError::Target {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
#[path = "portable_tests.rs"]
mod tests;
