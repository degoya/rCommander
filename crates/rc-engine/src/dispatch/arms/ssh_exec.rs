//! `Engine::dispatch` for the `ssh_exec` group (`registry/ssh_exec.rs`).

/// The `ssh_exec` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::SshExec(request) => $self.ssh_exec(&request).await,
        } }
    };
}
pub(crate) use arms;
