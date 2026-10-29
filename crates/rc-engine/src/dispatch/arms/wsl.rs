//! `Engine::dispatch` for the `wsl` group (`registry/wsl.rs`).

/// The `wsl` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::ListWslDistros => $self.wsl_distros().await,
            Command::RefreshWslDistros => $self.wsl_refresh().await,
        } }
    };
}
pub(crate) use arms;
