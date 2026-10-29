//! The command line under the panels (RC-010-08): what a typed line means, before anything runs.
//!
//! As in Total Commander, `cd` is the window's own: `cd <path>` (also `chdir`, `cd..`, `cd\`,
//! cmd's `cd /d <path>`, a quoted path) moves the active panel, and a drive alone (`d:`, or
//! `cd d:`) changes to the folder the panel last showed on that drive - no process starts. Every
//! other line runs in the system's shell in the panel's folder (`commands/command_line.rs`).
//!
//! A line is checked at the boundary ([`check`]): blank, longer than [`MAX_LINE`] characters, or
//! holding a control character (a line break would start a second command) is refused, and no
//! error ever repeats the line - it may hold a password. [`quote`] is how Ctrl+Enter writes a
//! name with blanks into the line.

use std::borrow::Cow;

use rc_vfs::BackendId;

use crate::{EngineError, VfsPath};

/// The longest line that runs: what fits into a `cmd.exe` command line with its own switches.
pub const MAX_LINE: usize = 8000;

/// Whether a command line runs in `folder`: only in a local one, not inside an archive or on a
/// server.
#[must_use]
pub fn runs_in(folder: &VfsPath) -> bool {
    folder.backend == BackendId::Local
}

/// What a line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed<'a> {
    /// `cd` without a folder: the panel stays.
    Stay,
    /// A drive alone, upper case: the panel changes to it.
    Drive(char),
    /// `cd` with a folder, absolute or relative to the panel's.
    Cd(String),
    /// Anything else: a command for the system's shell, trimmed.
    Run(&'a str),
}

/// `line` trimmed, when it may run.
///
/// # Errors
///
/// [`EngineError::CommandLineInvalid`]: the line is blank, too long or holds a control character.
pub fn check(line: &str) -> Result<&str, EngineError> {
    let line = line.trim();
    let reason = if line.is_empty() {
        "empty"
    } else if line.chars().count() > MAX_LINE {
        "longer than 8000 characters"
    } else if line.chars().any(char::is_control) {
        "control characters"
    } else {
        return Ok(line);
    };
    Err(EngineError::CommandLineInvalid { reason })
}

/// What the (checked) `line` asks for.
#[must_use]
pub fn parse(line: &str) -> Parsed<'_> {
    let line = line.trim();
    if let Some(letter) = drive_letter(line) {
        return Parsed::Drive(letter);
    }
    // `cd x && dir` is a command for the shell, where the `cd` stays inside the command.
    let Some(rest) = cd_argument(line).filter(|rest| !rest.contains(['&', '|', '<', '>'])) else {
        return Parsed::Run(line);
    };
    let target = unquote(without_drive_switch(rest.trim()));
    if target.is_empty() {
        return Parsed::Stay;
    }
    if let Some(letter) = drive_letter(target) {
        return Parsed::Drive(letter);
    }
    Parsed::Cd(native(target))
}

/// The letter of a drive written alone (`d:`, `D:`).
fn drive_letter(text: &str) -> Option<char> {
    rc_vfs::drive_letter(text)
        .filter(|_| text.len() == 2)
        .map(|letter| letter.to_ascii_uppercase())
}

/// What follows `cd` or `chdir` (any case) when the line is one: the word ends the line, or a
/// blank, `.`, `\` or `/` follows it (`cd..`, `cd\` as cmd takes them).
fn cd_argument(line: &str) -> Option<&str> {
    ["chdir", "cd"].into_iter().find_map(|word| {
        let head = line.get(..word.len())?;
        let rest = &line[word.len()..];
        let ends = rest
            .chars()
            .next()
            .is_none_or(|c| c.is_whitespace() || matches!(c, '.' | '\\' | '/'));
        (head.eq_ignore_ascii_case(word) && ends).then_some(rest)
    })
}

/// cmd's `/d` switch, which changes the drive too - what the panel does anyway.
fn without_drive_switch(text: &str) -> &str {
    match text.get(..2) {
        Some(switch) if switch.eq_ignore_ascii_case("/d") => {
            let rest = &text[2..];
            if rest.is_empty() || rest.starts_with(char::is_whitespace) {
                rest.trim_start()
            } else {
                text
            }
        }
        _ => text,
    }
}

/// `text` without the double quotes around it.
fn unquote(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(text)
        .trim()
}

/// A folder as this system writes it: `cd \` means the root on Linux and macOS too.
fn native(target: &str) -> String {
    if !cfg!(windows) && target == "\\" {
        "/".to_owned()
    } else {
        target.to_owned()
    }
}

/// The shell a line runs in, and so how a name is quoted for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    /// `cmd.exe`: double quotes around a name with blanks or one of its special characters.
    Cmd,
    /// `sh`: single quotes around anything but plain characters.
    Posix,
}

impl Shell {
    /// This system's shell.
    #[must_use]
    pub fn native() -> Self {
        if cfg!(windows) {
            Self::Cmd
        } else {
            Self::Posix
        }
    }
}

/// `text` (a name or a path) as this system's shell reads it as one word.
#[must_use]
pub fn quote(text: &str) -> Cow<'_, str> {
    quote_for(text, Shell::native())
}

/// `text` as `shell` reads it as one word: unchanged when it needs no quotes.
#[must_use]
pub fn quote_for(text: &str, shell: Shell) -> Cow<'_, str> {
    match shell {
        Shell::Cmd => {
            let special = |c: char| c.is_whitespace() || "&()[]{}^=;!'+,`~%".contains(c);
            if text.is_empty() || text.chars().any(special) {
                // A Windows name holds no double quote, so none needs escaping.
                Cow::Owned(format!("\"{text}\""))
            } else {
                Cow::Borrowed(text)
            }
        }
        Shell::Posix => {
            let plain = |c: char| c.is_ascii_alphanumeric() || "_-./:@%+=,".contains(c);
            if !text.is_empty() && text.chars().all(plain) {
                Cow::Borrowed(text)
            } else {
                Cow::Owned(format!("'{}'", text.replace('\'', "'\\''")))
            }
        }
    }
}

#[cfg(test)]
#[path = "command_line_tests.rs"]
mod tests;
