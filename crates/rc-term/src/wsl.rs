//! WSL shells: `wsl.exe -d <distro> --cd <linux path>` in a local PTY.
//!
//! Under Windows a shell in a WSL distribution is a local program, so it runs through the same
//! [`crate::spawn_local`] back end as `pwsh` and `cmd`; what it needs on top is the choice of the
//! distribution ([`choose_distro`], from the names WSL detection lists) and the folder as a
//! Linux path ([`crate::windows_to_wsl_path`]). The command line is built as separate arguments,
//! never as a shell string: the distribution is one of the detected names and the folder starts
//! with `/` or is `~`, so neither can be read by `wsl.exe` as an option.
//!
//! Outside Windows there is no WSL: [`wsl_profiles`] lists none and [`TermSession::open_wsl`]
//! answers [`TermError::WslUnavailable`].

use std::path::{Path, PathBuf};

use rc_vfs::parse_wsl_unc;

use crate::{
    DEFAULT_SCROLLBACK, Integration, LocalCommand, Platform, SessionOptions, ShellEnv, ShellPaths,
    ShellProfile, ShellSyntax, TermError, TermId, TermSession, TermSize,
    shells::{is_program, on_path},
    wsl_path::WslCwd,
};

/// The shell id of every WSL profile; the profile's `distro` tells them apart.
pub const WSL_SHELL: &str = "wsl";

/// The program, as Windows names it.
const WSL_EXE: &str = "wsl.exe";

/// A WSL shell to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WslLaunch {
    /// `wsl.exe`, a full path where it was found ([`wsl_program`]).
    pub program: PathBuf,
    /// The distribution, a name WSL detection listed; WSL's default when `None`.
    pub distro: Option<String>,
    /// The folder it starts in.
    pub cwd: WslCwd,
    /// How its login shell reads the `cd` the app types ([`crate::wsl_login_syntax`]).
    pub syntax: ShellSyntax,
}

impl WslLaunch {
    /// The program and its arguments, each argument on its own; what it starts ends with it.
    ///
    /// # Errors
    ///
    /// [`TermError::UnknownDistro`] for a name `wsl.exe` would take for an option or that
    /// cannot be a distribution's.
    pub fn command(&self) -> Result<LocalCommand, TermError> {
        let mut args = Vec::new();
        if let Some(distro) = &self.distro {
            if !plausible_name(distro) {
                return Err(TermError::UnknownDistro(distro.clone()));
            }
            args.push("-d".to_owned());
            args.push(distro.clone());
        }
        args.push("--cd".to_owned());
        args.push(self.cwd.as_arg().to_owned());
        Ok(LocalCommand {
            program: self.program.clone(),
            args,
            cwd: None,
            env: Vec::new(),
            end_programs_on_close: true,
        })
    }
}

/// Whether `name` can reach `wsl.exe` as a distribution's name and nothing else.
pub(crate) fn plausible_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '"')
}

/// `wsl.exe` in `%SystemRoot%\System32` (`SystemRoot` only when absolute, else `C:\Windows`),
/// else in an absolute `PATH` folder, else the System32 path all the same - never the bare name
/// (RC-080-65), so a `wsl.exe` in the current folder is never started instead, and a missing
/// one fails with its path.
#[must_use]
pub fn wsl_program(env: &ShellEnv) -> PathBuf {
    let platform = Platform::Windows;
    let in_system32 = rc_vfs::system32(env.system_root.as_deref()).join(WSL_EXE);
    if is_program(platform, &in_system32) {
        return in_system32;
    }
    on_path(platform, env, WSL_EXE).unwrap_or(in_system32)
}

/// One profile "WSL: <name>" per detected distribution, in the order detection lists them;
/// none outside Windows or without a distribution.
#[must_use]
pub fn wsl_profiles(platform: Platform, env: &ShellEnv, distros: &[String]) -> Vec<ShellProfile> {
    if platform != Platform::Windows || distros.is_empty() {
        return Vec::new();
    }
    let program = wsl_program(env);
    distros
        .iter()
        .map(|distro| ShellProfile {
            id: WSL_SHELL.to_owned(),
            label_key: "terminal.shell.wsl".to_owned(),
            program: program.clone(),
            args: vec!["-d".to_owned(), distro.clone()],
            distro: Some(distro.clone()),
        })
        .collect()
}

/// The distribution to start: the installed one `asked` names (ignoring case, spelled as WSL
/// spells it), or `None` for WSL's default.
///
/// # Errors
///
/// [`TermError::WslUnavailable`] when `installed` is empty (WSL missing, switched off or
/// without a distribution), [`TermError::UnknownDistro`] when `asked` is not installed.
pub fn choose_distro(
    installed: &[String],
    asked: Option<&str>,
) -> Result<Option<String>, TermError> {
    if installed.is_empty() {
        return Err(TermError::WslUnavailable);
    }
    let Some(asked) = asked else {
        return Ok(None);
    };
    installed
        .iter()
        .find(|name| name.eq_ignore_ascii_case(asked))
        .map(|name| Some(name.clone()))
        .ok_or_else(|| TermError::UnknownDistro(asked.to_owned()))
}

/// The shell a new terminal runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellChoice {
    /// A shell of this machine: the profile by its id, the default profile when `None`.
    Local(Option<String>),
    /// A WSL shell of the distribution by its name, WSL's default when `None`.
    Wsl(Option<String>),
}

/// The shell for a terminal in `folder` from the `shell` and `distro` asked for.
///
/// What is asked wins: a shell id other than [`WSL_SHELL`] is that local shell, a distribution
/// alone is its WSL shell. Asked for nothing, a folder in a distribution's share
/// (`\\wsl.localhost\<distro>\...`, `\\wsl$\<distro>\...`) opens that distribution's shell and
/// every other folder the default local shell; [`WSL_SHELL`] without a distribution takes the
/// share's distribution too. The name is the share's spelling; [`choose_distro`] matches it
/// against the installed ones ignoring case.
#[must_use]
pub fn choose_shell(shell: Option<&str>, distro: Option<&str>, folder: &Path) -> ShellChoice {
    let share = || parse_wsl_unc(folder).map(|inside| inside.distro);
    match (shell, distro) {
        (Some(WSL_SHELL), _) => ShellChoice::Wsl(distro.map(str::to_owned).or_else(share)),
        (Some(name), _) => ShellChoice::Local(Some(name.to_owned())),
        (None, Some(asked)) => ShellChoice::Wsl(Some(asked.to_owned())),
        (None, None) => share().map_or(ShellChoice::Local(None), |name| {
            ShellChoice::Wsl(Some(name))
        }),
    }
}

impl TermSession {
    /// A WSL shell of `launch` in a new PTY; the session keeps `folder` (the panel's Windows
    /// folder) as its folder and is titled after the distribution.
    ///
    /// # Errors
    ///
    /// [`TermError::WslUnavailable`] outside Windows, [`TermError::UnknownDistro`] for a
    /// distribution name that is no name, [`TermError::Pty`] when `wsl.exe` cannot be started.
    pub fn open_wsl(
        id: TermId,
        launch: &WslLaunch,
        folder: PathBuf,
        size: TermSize,
    ) -> Result<Self, TermError> {
        Self::open_wsl_with(id, launch, folder, size, None, true)
    }

    /// [`Self::open_wsl`] with the shell integration's variables when given; the session
    /// follows the panel in the launch's syntax with the distribution's paths. `end_programs`
    /// is [`LocalCommand::end_programs_on_close`].
    ///
    /// # Errors
    ///
    /// As [`Self::open_wsl`].
    pub fn open_wsl_with(
        id: TermId,
        launch: &WslLaunch,
        folder: PathBuf,
        size: TermSize,
        integration: Option<&Integration>,
        end_programs: bool,
    ) -> Result<Self, TermError> {
        let mut command = launch.command()?;
        command.end_programs_on_close = end_programs;
        if let Some(integration) = integration {
            integration.apply(&mut command);
        }
        let spawned = spawn(&command, size)?;
        let title = launch.distro.clone().unwrap_or_else(|| WSL_EXE.to_owned());
        let session = Self::start(
            id,
            spawned,
            SessionOptions {
                title,
                shell: WSL_SHELL.to_owned(),
                cwd: folder,
                size,
                scrollback: DEFAULT_SCROLLBACK,
            },
        );
        let mut sync = session.sync();
        sync.configure(Some(launch.syntax), ShellPaths::Wsl(launch.distro.clone()));
        sync.require_nonce(integration.map(|integration| integration.nonce.clone()));
        drop(sync);
        Ok(session)
    }
}

#[cfg(windows)]
fn spawn(command: &LocalCommand, size: TermSize) -> Result<crate::Spawned, TermError> {
    crate::spawn_local(command, size)
}

#[cfg(not(windows))]
fn spawn(_command: &LocalCommand, _size: TermSize) -> Result<crate::Spawned, TermError> {
    Err(TermError::WslUnavailable)
}

#[cfg(test)]
#[path = "wsl_tests.rs"]
mod tests;
