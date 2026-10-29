//! `Engine::dispatch` for the `command_line` group (`registry/command_line.rs`).

/// The `command_line` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::RunCommandLine {
                panel,
                line,
                keep_open,
            } => $self.run_command_line(panel, &line, keep_open).await,
            Command::Filter {
                panel,
                pattern,
                last,
            } => $self.filter(panel, pattern.as_deref(), last),
        } }
    };
}
pub(crate) use arms;
