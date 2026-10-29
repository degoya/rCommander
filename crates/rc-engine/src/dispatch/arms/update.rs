//! `Engine::dispatch` for the `update` group (`registry/update.rs`).

/// The `update` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::GetUpdateStatus {} => $self.update_status_outcome().await,
            Command::CheckForUpdates {} => $self.check_for_updates().await,
            Command::DownloadUpdate {} => $self.download_update().await,
            Command::InstallUpdate {} => $self.install_update().await,
        } }
    };
}
pub(crate) use arms;
