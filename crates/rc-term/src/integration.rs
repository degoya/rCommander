//! The shell integration the app hands a shell at its start (RC-040-04): a few lines that report
//! the folder (OSC 7, OSC 9;9) and mark the prompt (OSC 133) before every prompt, so the panel
//! can follow the shell and the shell the panel (`cwd_sync.rs`).
//!
//! Nothing is typed into the shell for this; each shell gets it through its own start-up:
//! - PowerShell: `-NoExit -EncodedCommand` with `pwsh.ps1`, which wraps the profile's prompt;
//! - bash (local and in WSL): `PROMPT_COMMAND` evaluates `bash.sh` from a variable, handed into
//!   WSL through `WSLENV`;
//! - zsh: `ZDOTDIR` points at a folder of the app's with `zsh.zsh` as `.zshenv`, which hands
//!   `ZDOTDIR` back before zsh reads the user's own files;
//! - cmd: the `PROMPT` variable writes OSC 9;9 and the prompt marks (`$E` is ESC). cmd marks
//!   only its prompt, not a command's start; the session counts Enter as leaving the prompt.
//!
//! The snippets are documented for adding by hand (the wiki's terminal page); a shell without
//! them simply does not follow, and the app types nothing into it.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use base64::Engine as _;

use crate::{
    DEFAULT_SCROLLBACK, LocalCommand, Platform, SessionOptions, ShellPaths, ShellProfile,
    ShellSyntax, TermError, TermId, TermSession, TermSize, spawn_local,
};

/// PowerShell's snippet.
pub const PWSH_SNIPPET: &str = include_str!("integration/pwsh.ps1");
/// bash's snippet.
pub const BASH_SNIPPET: &str = include_str!("integration/bash.sh");
/// zsh's snippet, a `.zshenv`.
pub const ZSH_SNIPPET: &str = include_str!("integration/zsh.zsh");

/// The variable that holds bash's snippet.
const BASH_VARIABLE: &str = "RC_SHELL_INTEGRATION";
/// What `PROMPT_COMMAND` runs.
const BASH_PROMPT_COMMAND: &str = "eval \"$RC_SHELL_INTEGRATION\"";
/// cmd's prompt around the user's own: the folder and the start mark before, the end mark after.
const CMD_BEFORE: &str = r"$E]9;9;$P$E\$E]133;A$E\";
const CMD_AFTER: &str = r"$E]133;B$E\";
/// cmd's prompt when the user has none.
const CMD_DEFAULT_PROMPT: &str = "$P$G";

/// The variables of this process the integration builds on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InheritedEnv {
    /// `PROMPT` (cmd).
    pub prompt: Option<String>,
    /// `PROMPT_COMMAND` (bash).
    pub prompt_command: Option<String>,
    /// `ZDOTDIR` (zsh).
    pub zdotdir: Option<String>,
    /// `WSLENV`.
    pub wslenv: Option<String>,
}

impl InheritedEnv {
    /// This process's environment.
    #[must_use]
    pub fn from_env() -> Self {
        let var = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
        Self {
            prompt: var("PROMPT"),
            prompt_command: var("PROMPT_COMMAND"),
            zdotdir: var("ZDOTDIR"),
            wslenv: var("WSLENV"),
        }
    }
}

/// What a shell starts with to have the integration: arguments after its own, variables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Integration {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl Integration {
    /// For the local shell of profile id `shell`; `zsh_dir` is the folder [`zsh_dotdir`]
    /// wrote. `None` for a shell without one (sh, fish, other shells, zsh without its folder).
    #[must_use]
    pub fn local(shell: &str, inherited: &InheritedEnv, zsh_dir: Option<&Path>) -> Option<Self> {
        match shell {
            "pwsh" | "powershell" => Some(Self {
                args: vec![
                    "-NoExit".to_owned(),
                    "-EncodedCommand".to_owned(),
                    encoded_command(PWSH_SNIPPET),
                ],
                env: Vec::new(),
            }),
            "bash" => Some(Self {
                args: Vec::new(),
                env: bash_env(inherited),
            }),
            "zsh" => zsh_dir.map(|dir| {
                let mut env = vec![("ZDOTDIR".to_owned(), dir.to_string_lossy().into_owned())];
                if let Some(own) = &inherited.zdotdir {
                    env.push(("RC_USER_ZDOTDIR".to_owned(), own.clone()));
                }
                Self {
                    args: Vec::new(),
                    env,
                }
            }),
            "cmd" => {
                let own = inherited.prompt.as_deref().unwrap_or(CMD_DEFAULT_PROMPT);
                Some(Self {
                    args: Vec::new(),
                    env: vec![("PROMPT".to_owned(), format!("{CMD_BEFORE}{own}{CMD_AFTER}"))],
                })
            }
            _ => None,
        }
    }

    /// For a WSL shell: bash's variables, handed into the distribution through `WSLENV` (a
    /// login shell other than bash ignores them).
    #[must_use]
    pub fn wsl(inherited: &InheritedEnv) -> Self {
        let mut env = bash_env(&InheritedEnv::default());
        let names: Vec<String> = env.iter().map(|(name, _)| format!("{name}/u")).collect();
        let wslenv = match &inherited.wslenv {
            Some(own) => format!("{own}:{}", names.join(":")),
            None => names.join(":"),
        };
        env.push(("WSLENV".to_owned(), wslenv));
        Self {
            args: Vec::new(),
            env,
        }
    }

    /// `command` with the integration added.
    pub fn apply(&self, command: &mut LocalCommand) {
        command.args.extend(self.args.iter().cloned());
        command.env.extend(self.env.iter().cloned());
    }
}

/// bash's variables: the snippet, and `PROMPT_COMMAND` evaluating it before the user's own.
fn bash_env(inherited: &InheritedEnv) -> Vec<(String, String)> {
    let prompt_command = match &inherited.prompt_command {
        Some(own) => format!("{BASH_PROMPT_COMMAND};{own}"),
        None => BASH_PROMPT_COMMAND.to_owned(),
    };
    vec![
        (BASH_VARIABLE.to_owned(), unix_lines(BASH_SNIPPET)),
        ("PROMPT_COMMAND".to_owned(), prompt_command),
    ]
}

/// A script for PowerShell's `-EncodedCommand`: UTF-16LE in base64, so no quoting of the
/// command line is involved.
fn encoded_command(script: &str) -> String {
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64::engine::general_purpose::STANDARD.encode(utf16)
}

/// `text` with LF line ends, whatever a checkout made of the file.
fn unix_lines(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// Writes zsh's `.zshenv` into `base/zsh` (a folder only the user can read, under the app's
/// data folder) when it is missing or differs; the folder for `ZDOTDIR`.
///
/// # Errors
///
/// The folder or the file cannot be written.
pub fn zsh_dotdir(base: &Path) -> io::Result<PathBuf> {
    let dir = base.join("zsh");
    fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    }
    let file = dir.join(".zshenv");
    let content = unix_lines(ZSH_SNIPPET);
    if fs::read_to_string(&file).ok().as_deref() != Some(content.as_str()) {
        fs::write(&file, content)?;
    }
    Ok(dir)
}

impl TermSession {
    /// A local shell of `profile` in a new PTY, with `integration` when given; the session
    /// follows the panel in the syntax of the profile's shell. `end_programs` is
    /// [`LocalCommand::end_programs_on_close`].
    ///
    /// # Errors
    ///
    /// [`TermError::Pty`] when the PTY cannot be opened or the shell not started.
    pub fn open_local_with(
        id: TermId,
        profile: &ShellProfile,
        cwd: PathBuf,
        size: TermSize,
        integration: Option<&Integration>,
        end_programs: bool,
    ) -> Result<Self, TermError> {
        let mut command = LocalCommand {
            program: profile.program.clone(),
            args: profile.args.clone(),
            cwd: Some(cwd.clone()),
            env: Vec::new(),
            end_programs_on_close: end_programs,
        };
        if let Some(integration) = integration {
            integration.apply(&mut command);
        }
        let spawned = spawn_local(&command, size)?;
        let title = profile
            .program
            .file_name()
            .map_or_else(|| profile.id.clone(), |name| name.to_string_lossy().into());
        let session = Self::start(
            id,
            spawned,
            SessionOptions {
                title,
                shell: profile.id.clone(),
                cwd,
                size,
                scrollback: DEFAULT_SCROLLBACK,
            },
        );
        session.sync().configure(
            ShellSyntax::of(&profile.id),
            ShellPaths::Native(Platform::CURRENT),
        );
        Ok(session)
    }
}

#[cfg(test)]
#[path = "integration_tests.rs"]
mod tests;
