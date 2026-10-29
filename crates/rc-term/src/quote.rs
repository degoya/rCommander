//! The one line the app types into a shell: a `cd` to the panel's folder (RC-040-04).
//!
//! A folder name is data, never code: whatever it holds (quotes, `;`, `&`, `|`, `$(...)`,
//! backticks, `%VAR%`) must reach the shell as the name and run nothing. Each syntax quotes it
//! the way its shell reads a literal; a name that cannot be quoted that safely - a control
//! character in any shell, `"`, `%` or `!` in `cmd` - gets no line at all.

use serde::{Deserialize, Serialize};

/// How a shell reads its command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellSyntax {
    /// `sh`, `bash`, `zsh` (also in WSL): single quotes, nothing inside is special.
    Posix,
    /// PowerShell 7 and Windows PowerShell: single quotes, a quote doubled.
    PowerShell,
    /// `cmd.exe`: double quotes; `%` expands even inside them.
    Cmd,
}

impl ShellSyntax {
    /// The syntax of a shell profile's id; `None` for a shell the app does not type into (fish,
    /// unknown shells).
    #[must_use]
    pub fn of(shell: &str) -> Option<Self> {
        match shell {
            "sh" | "bash" | "zsh" | "dash" | "ksh" | "wsl" => Some(Self::Posix),
            "pwsh" | "powershell" => Some(Self::PowerShell),
            "cmd" => Some(Self::Cmd),
            _ => None,
        }
    }
}

/// The characters PowerShell takes for a single quote: `'` and the typographic ones.
const POWERSHELL_QUOTES: &[char] = &['\'', '\u{2018}', '\u{2019}', '\u{201a}', '\u{201b}'];

/// The line that moves the shell to `path`, Enter included, or `None` when `path` cannot be
/// quoted safely. It starts with a space, which keeps it out of a history that ignores such
/// lines (bash's `ignorespace`, zsh's `HIST_IGNORE_SPACE`).
#[must_use]
pub fn cd_line(syntax: ShellSyntax, path: &str) -> Option<String> {
    if path.is_empty() || path.chars().any(char::is_control) {
        return None;
    }
    let line = match syntax {
        ShellSyntax::Posix => format!(" cd -- '{}'", path.replace('\'', r"'\''")),
        ShellSyntax::PowerShell => {
            let mut quoted = String::with_capacity(path.len() + 2);
            for c in path.chars() {
                if POWERSHELL_QUOTES.contains(&c) {
                    quoted.push(c);
                }
                quoted.push(c);
            }
            format!(" Set-Location -LiteralPath '{quoted}'")
        }
        ShellSyntax::Cmd => {
            if path.contains(['"', '%', '!']) {
                return None;
            }
            format!(" cd /d \"{path}\"")
        }
    };
    Some(line + "\r")
}

#[cfg(test)]
#[path = "quote_tests.rs"]
mod tests;
