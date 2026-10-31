//! `Engine::dispatch` for the `theme` group (`registry/theme.rs`).

/// The `theme` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::DeleteTheme { id } => $self.delete_theme(&id).await,
            Command::ListThemes {} => $self.list_themes().await,
            Command::SaveTheme { theme } => $self.save_theme(&theme).await,
            Command::SelectTheme { mode, id } => $self.select_theme(mode, &id).await,
        } }
    };
}
pub(crate) use arms;
