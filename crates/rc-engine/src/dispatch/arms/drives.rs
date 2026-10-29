//! `Engine::dispatch` for the `drives` group (`registry/drives.rs`).

/// The `drives` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::ListDrives { refresh } => Ok($self.list_drives(refresh).await),
            Command::MapNetworkDrive {
                remote,
                letter,
                reconnect,
            } => $self.map_network_drive(&remote, &letter, reconnect).await,
            Command::UnmapNetworkDrive { drive, force } => {
                $self.unmap_network_drive(&drive, force).await
            }
        } }
    };
}
pub(crate) use arms;
