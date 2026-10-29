//! `Engine::dispatch` for the `remote` group (`registry/remote.rs`).

/// The `remote` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::QuickConnect(request) => $self.quick_connect(request).await,
            Command::Stat { panel, path } => $self.stat_entry(panel, &path).await,
        } }
    };
}
pub(crate) use arms;
