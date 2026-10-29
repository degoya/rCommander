//! The terminal's errors, each with a stable code.

use crate::TermId;

/// What went wrong with a terminal.
///
/// [`TermError::code`] is the stable machine-readable form, the i18n key's last part
/// (`error.<code>`) and what MCP answers. A code never changes once released.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TermError {
    /// No open terminal has this id.
    #[error("no terminal with id {0}")]
    NotFound(TermId),
    /// The terminal's program has ended; it takes no input and no new size.
    #[error("the terminal's program has ended")]
    Exited,
    /// No detected shell has this id.
    #[error("no shell named {0:?} was found on this machine")]
    UnknownShell(String),
    /// A size of zero rows or columns, or of more than `TermSize::MAX`.
    #[error("a terminal needs 1 to 1000 rows and 1 to 1000 columns")]
    InvalidSize,
    /// The pseudo-terminal could not be opened, or the program not started in it.
    #[error("the terminal could not be started: {0}")]
    Pty(String),
    /// WSL is not installed, switched off or has no distribution (and never outside Windows).
    #[error("WSL is not available on this machine")]
    WslUnavailable,
    /// No installed WSL distribution has this name.
    #[error("no WSL distribution named {0:?} is installed")]
    UnknownDistro(String),
    /// Typed input is held: a login script talks to the program until it ends.
    #[error("a login script is running; the terminal takes input when it ends (Ctrl+C stops it)")]
    Busy,
}

impl TermError {
    /// Every code, in declaration order.
    pub const ALL_CODES: &[&str] = &[
        "terminal_not_found",
        "terminal_exited",
        "shell_not_found",
        "terminal_invalid_size",
        "terminal_start_failed",
        "wsl_unavailable",
        "wsl_distro_not_found",
        "terminal_busy",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "terminal_not_found",
            Self::Exited => "terminal_exited",
            Self::UnknownShell(_) => "shell_not_found",
            Self::InvalidSize => "terminal_invalid_size",
            Self::Pty(_) => "terminal_start_failed",
            Self::WslUnavailable => "wsl_unavailable",
            Self::UnknownDistro(_) => "wsl_distro_not_found",
            Self::Busy => "terminal_busy",
        }
    }
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
