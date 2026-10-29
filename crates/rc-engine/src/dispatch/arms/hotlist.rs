//! `Engine::dispatch` for the `hotlist` group (`registry/hotlist.rs`).

/// The `hotlist` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::ListHotlist {} => $self.list_hotlist().await,
            Command::AddHotlist(request) => $self.add_hotlist(request).await,
            Command::AddHotlistFolder(request) => $self.add_hotlist_folder(request).await,
            Command::UpdateHotlist(request) => $self.update_hotlist(request).await,
            Command::MoveHotlist(request) => $self.move_hotlist(request).await,
            Command::DeleteHotlist { id } => $self.delete_hotlist(id).await,
            Command::OpenHotlist(request) => $self.open_hotlist(request).await,
        } }
    };
}
pub(crate) use arms;
