//! rCommander's terminal sessions, without a GUI.
//!
//! A terminal is two halves. The [`TermBackend`] carries bytes to and from a program: a local
//! shell in a pseudo-terminal ([`spawn_local`], ConPTY on Windows, openpty elsewhere), a shell in
//! a WSL distribution (`wsl.exe` in the same kind of PTY, [`TermSession::open_wsl`], its folder
//! translated by [`windows_to_wsl_path`]), and a shell on an SSH channel ([`spawn_ssh`],
//! RC-060-03: the channel of the connection's pooled session, so an SFTP panel and a terminal
//! on one server share a TCP connection). The [`TermSession`] feeds the program's output
//! through the VT parser into an `alacritty_terminal` [`Term`], answers the terminal's own
//! replies through the back end, and keeps the session's id, title, folder, size and state. The screen reads back as text
//! ([`ScreenText`]) for MCP; the panel widget (`rc-term-widget`, the iced_term fork of RC-040-03)
//! renders the same [`Term`] through [`TermSession::term`].
//!
//! Every back end has the same shape - bytes in ([`TermBackend::write`]), bytes out and the end
//! ([`BackendEvent`] on a channel), a size ([`TermBackend::resize`]) - so the session, the
//! screen and the widget never know which one they talk to. A [`Tap`] copies a back end's output
//! for a login script that talks to the program while the session holds the person's input
//! ([`TermSession::hold_input`]).
//!
//! The session also keeps the shell's folder and prompt (RC-040-04): the program's OSC 7, OSC
//! 9;9 and OSC 133 ([`osc`]) tell where the shell is and whether it waits at its prompt, and
//! [`TermSession::follow`] types a `cd` ([`cd_line`]) only then. The shell integration that makes
//! a shell say so is handed over at its start ([`Integration`]).
//!
//! [`Term`]: alacritty_terminal::Term

mod backend;
mod cwd_sync;
mod error;
mod integration;
mod local_pty;
pub mod osc;
mod quote;
mod screen;
mod session;
mod shells;
mod tap;
#[cfg(test)]
mod test_support;
mod wsl;
mod wsl_path;

pub use alacritty_terminal;
pub use backend::{BackendEvent, Spawned, SshTermChannel, TermBackend, TermSize, spawn_ssh};
pub use cwd_sync::{FollowOutcome, FollowStatus, PromptState};
pub use error::TermError;
pub use integration::{
    BASH_SNIPPET, InheritedEnv, Integration, PWSH_SNIPPET, ZSH_SNIPPET, zsh_dotdir,
};
pub use local_pty::{LocalCommand, spawn_local};
pub use quote::{ShellSyntax, cd_line};
pub use screen::{CursorPos, ScreenText, screen_text};
pub use session::{
    DEFAULT_SCROLLBACK, EventProxy, InputHook, SessionInfo, SessionOptions, SessionState,
    TermEvent, TermId, TermSession,
};
pub use shells::{Platform, ShellEnv, ShellProfile, detect_shells, local_shells};
pub use tap::{TAP_QUEUE, Tap, tap};
pub use wsl::{
    ShellChoice, WSL_SHELL, WslLaunch, choose_distro, choose_shell, wsl_profiles, wsl_program,
};
pub use wsl_path::{
    HomeReason, ShellPaths, WslCwd, panel_to_shell_path, shell_fits, shell_to_panel_path,
    windows_to_wsl_path,
};
