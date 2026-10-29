//! `Engine::dispatch` for the `backup` group (`registry/backup.rs`).

/// The `backup` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::BackupDatabase => $self.backup_database().await,
            Command::BackupDatabaseTo(crate::BackupToRequest { path, overwrite }) => {
                $self.backup_database_to(&path, overwrite).await
            }
            Command::RestoreDatabase { path } => $self.restore_database(&path).await,
        } }
    };
}
pub(crate) use arms;
