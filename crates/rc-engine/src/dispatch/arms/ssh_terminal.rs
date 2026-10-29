//! `Engine::dispatch` for the `ssh_terminal` group (`registry/ssh_terminal.rs`).

/// The `ssh_terminal` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::OpenSshTerminal {
                connection_id,
                panel,
                cwd,
                size,
            } => {
                $self.open_ssh_terminal(panel, connection_id, cwd, size)
                    .await
            }
        } }
    };
}
pub(crate) use arms;
