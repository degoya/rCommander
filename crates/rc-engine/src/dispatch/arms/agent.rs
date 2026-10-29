//! `Engine::dispatch` for the `agent` group (`registry/agent.rs`).

/// The `agent` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::ListAgentIdentities => $self.list_agent_identities().await,
        } }
    };
}
pub(crate) use arms;
