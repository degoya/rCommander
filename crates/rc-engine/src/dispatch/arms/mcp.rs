//! `Engine::dispatch` for the `mcp` group (`registry/mcp.rs`).

/// The `mcp` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::AnswerMcpQuestion { question, answer } => {
                $self.answer_mcp_question(question, answer)
            }
            Command::RevokeMcpGrants {} => $self.revoke_mcp_grants(),
        } }
    };
}
pub(crate) use arms;
