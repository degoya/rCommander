//! `Engine::dispatch` for the `terminal` group (`registry/terminal.rs`).

/// The `terminal` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::CloseTerminal { terminal } => $self.close_terminal(terminal).await,
            Command::ListTerminals {} => Ok($self.list_terminals().await),
            Command::MaximizeTerminal { panel } => $self.maximize_terminal(panel).await,
            Command::OpenTerminal {
                panel,
                shell,
                distro,
                cwd,
                size,
            } => {
                $self.open_terminal(panel, shell, distro, cwd.as_deref(), size)
                    .await
            }
            Command::PasteImage { terminal } => Ok($self.paste_image(terminal)),
            Command::ReadScreen {
                terminal,
                scrollback,
            } => $self.read_screen(terminal, scrollback),
            Command::SendInput { terminal, text } => $self.send_input(terminal, text),
            Command::ShowTerminal { terminal } => $self.show_terminal(terminal).await,
        } }
    };
}
pub(crate) use arms;
