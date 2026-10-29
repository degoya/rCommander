//! `Engine::dispatch` for the `settings` group (`registry/settings.rs`).

/// The `settings` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::ListKeymap => $self.list_keymap().await,
            Command::SetKeymap { overrides } => $self.set_keymap(&overrides).await,
            Command::GetSettings { key } => $self.get_settings(key.as_deref()).await,
            Command::SetSetting { key, value } => $self.set_setting(&key, &value).await,
        } }
    };
}
pub(crate) use arms;
