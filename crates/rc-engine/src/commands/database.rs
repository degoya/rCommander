//! The `db.*` commands: backing up `rcommander.db` and staging a restore (RC-010-12).
//!
//! `db.backup` takes no path: it writes into `rc_db::BACKUP_DIR` beside the database, the one
//! place MCP may write a backup to, and then keeps the newest `backup.keep` backups under a
//! generated name there (RC-080-42); a backup a person named stays. A path a person chose
//! (`db.backup_to`) and a restore (`db.restore`) are `GuiOnly`.

use std::path::Path;

use serde_json::Value;

use crate::{
    Engine, EngineError, Outcome,
    settings::{BACKUP_KEEP_KEY, stored_setting},
};

impl Engine {
    /// `db.backup`: into the backup folder, under a generated name; then the oldest automatic
    /// backups beyond `backup.keep` are removed. A failure there is logged: the new backup
    /// stands.
    pub(crate) async fn backup_database(&self) -> Result<Outcome, EngineError> {
        let db = self.db()?;
        let report = db.backup_to_backup_dir().await?;
        let keep = stored_setting(db, BACKUP_KEEP_KEY)
            .await
            .as_ref()
            .and_then(Value::as_u64)
            .and_then(|keep| usize::try_from(keep).ok())
            .unwrap_or(*rc_db::BACKUP_KEEP_RANGE.end());
        let pruned = match db.backup_dir() {
            Ok(dir) => tokio::task::spawn_blocking(move || rc_db::prune_backups(&dir, keep))
                .await
                .map_err(|_| "the pruning task ended early".to_owned())
                .and_then(|pruned| pruned.map_err(|error| error.to_string())),
            Err(error) => Err(error.to_string()),
        };
        match pruned {
            Ok(removed) if !removed.is_empty() => {
                tracing::info!(removed = removed.len(), keep, "older backups removed");
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "older backups could not be removed"),
        }
        Ok(Outcome::DatabaseBackup(report))
    }

    /// `db.backup_to`: into a file, or a folder for a generated name.
    pub(crate) async fn backup_database_to(
        &self,
        path: &str,
        overwrite: bool,
    ) -> Result<Outcome, EngineError> {
        let report = self.db()?.backup_to(Path::new(path), overwrite).await?;
        Ok(Outcome::DatabaseBackup(report))
    }

    /// `db.restore`: checked now, staged for the next start after a backup of the current state.
    pub(crate) async fn restore_database(&self, path: &str) -> Result<Outcome, EngineError> {
        let report = self.db()?.restore_from(Path::new(path)).await?;
        Ok(Outcome::RestoreStaged(report))
    }
}

#[cfg(test)]
#[path = "database_tests.rs"]
mod tests;
