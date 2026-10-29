//! `Engine::dispatch` for the `manager` group (`registry/manager.rs`).

/// The `manager` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::ConnectionManager {} => Ok($self.connection_manager()),
            Command::SetFavourite { id, favourite } => $self.set_favourite(id, favourite).await,
        } }
    };
}
pub(crate) use arms;
