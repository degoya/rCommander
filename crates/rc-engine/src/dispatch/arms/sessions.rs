//! `Engine::dispatch` for the `sessions` group (`registry/sessions.rs`).

/// The `sessions` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::ConnectSession { id } => $self.connect_session(id, !from_mcp()).await,
            Command::DisconnectSession { id } => $self.disconnect_session(id).await,
            Command::TestConnection { id } => $self.test_session(id, !from_mcp()).await,
            Command::GetSessionPrefs { id } => $self.get_session_prefs(id).await,
            Command::SetSessionPrefs { id, prefs } => $self.set_session_prefs(id, &prefs).await,
        } }
    };
}
pub(crate) use arms;
