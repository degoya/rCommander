//! Which local shells this machine has, in the order the app offers them; the first is the
//! default.
//!
//! Windows: PowerShell 7 (`pwsh.exe`, on `PATH` or where its installers put it), Windows
//! PowerShell and `cmd.exe` - those two from `%SystemRoot%\System32` first, so a `cmd.exe` in a
//! folder early on `PATH` is not the shell (RC-080-43). Unix: the login shell from `$SHELL`, then
//! `bash`, `zsh` and `sh` from `PATH`. Only absolute `PATH` entries count: a relative one is
//! read against whatever folder the app runs in. The decision reads a [`ShellEnv`] and takes the [`Platform`] as a parameter, so
//! both halves are tested on any machine with a fake `PATH`.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// A shell the app can start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellProfile {
    /// Stable id: `pwsh`, `powershell`, `cmd`, `bash`, `zsh`, `sh`, `fish`, or the file name of
    /// another login shell.
    pub id: String,
    /// The i18n key of its name: `terminal.shell.<id>`, `terminal.shell.other` for a shell the
    /// catalogue does not name.
    pub label_key: String,
    /// The program, a full path when it was found; a bare name the system looks up otherwise.
    pub program: PathBuf,
    pub args: Vec<String>,
    /// The WSL distribution of a `wsl` profile; `None` for every other shell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distro: Option<String>,
}

impl ShellProfile {
    fn new(id: &str, program: PathBuf, args: &[&str]) -> Self {
        let label_key = if NAMED_SHELLS.contains(&id) {
            format!("terminal.shell.{id}")
        } else {
            "terminal.shell.other".to_owned()
        };
        Self {
            id: id.to_owned(),
            label_key,
            program,
            args: args.iter().map(|&arg| arg.to_owned()).collect(),
            distro: None,
        }
    }
}

/// The shells the i18n catalogue has a name for.
const NAMED_SHELLS: &[&str] = &["bash", "cmd", "fish", "powershell", "pwsh", "sh", "zsh"];

/// The two families of rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Unix,
}

impl Platform {
    /// The platform this build runs on.
    pub const CURRENT: Self = if cfg!(windows) {
        Self::Windows
    } else {
        Self::Unix
    };

    fn path_separator(self) -> char {
        match self {
            Self::Windows => ';',
            Self::Unix => ':',
        }
    }
}

/// What detection reads from the environment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellEnv {
    /// `PATH`.
    pub path: Option<OsString>,
    /// `SHELL` (Unix).
    pub shell: Option<OsString>,
    /// `ProgramFiles` (Windows).
    pub program_files: Option<PathBuf>,
    /// `LOCALAPPDATA` (Windows).
    pub local_app_data: Option<PathBuf>,
    /// `SystemRoot` (Windows).
    pub system_root: Option<PathBuf>,
}

impl ShellEnv {
    /// This process's environment.
    #[must_use]
    pub fn from_env() -> Self {
        let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
        Self {
            path: var("PATH"),
            shell: var("SHELL"),
            program_files: var("ProgramFiles").map(PathBuf::from),
            local_app_data: var("LOCALAPPDATA").map(PathBuf::from),
            system_root: var("SystemRoot").map(PathBuf::from),
        }
    }
}

/// The shells of this machine.
#[must_use]
pub fn local_shells() -> Vec<ShellProfile> {
    detect_shells(Platform::CURRENT, &ShellEnv::from_env())
}

/// The shells `env` offers on `platform`; never empty, the default first.
#[must_use]
pub fn detect_shells(platform: Platform, env: &ShellEnv) -> Vec<ShellProfile> {
    match platform {
        Platform::Windows => windows_shells(env),
        Platform::Unix => unix_shells(env),
    }
}

fn windows_shells(env: &ShellEnv) -> Vec<ShellProfile> {
    let platform = Platform::Windows;
    let mut shells = Vec::new();
    let installed_pwsh = [
        env.program_files
            .as_ref()
            .map(|dir| dir.join("PowerShell").join("7").join("pwsh.exe")),
        env.program_files
            .as_ref()
            .map(|dir| dir.join("PowerShell").join("7-preview").join("pwsh.exe")),
        env.local_app_data
            .as_ref()
            .map(|dir| dir.join("Microsoft").join("WindowsApps").join("pwsh.exe")),
    ];
    if let Some(pwsh) = on_path(platform, env, "pwsh.exe").or_else(|| {
        installed_pwsh
            .into_iter()
            .flatten()
            .find(|candidate| is_program(platform, candidate))
    }) {
        shells.push(ShellProfile::new("pwsh", pwsh, &["-NoLogo"]));
    }
    let system32 = env
        .system_root
        .as_ref()
        .filter(|root| root.is_absolute())
        .map(|root| root.join("System32"));
    let in_system32 = |relative: &[&str]| {
        system32
            .as_ref()
            .map(|dir| {
                relative
                    .iter()
                    .fold(dir.clone(), |path, part| path.join(part))
            })
            .filter(|candidate| is_program(platform, candidate))
    };
    let powershell = in_system32(&["WindowsPowerShell", "v1.0", "powershell.exe"])
        .or_else(|| on_path(platform, env, "powershell.exe"))
        .unwrap_or_else(|| PathBuf::from("powershell.exe"));
    shells.push(ShellProfile::new("powershell", powershell, &["-NoLogo"]));
    let cmd = in_system32(&["cmd.exe"])
        .or_else(|| on_path(platform, env, "cmd.exe"))
        .unwrap_or_else(|| PathBuf::from("cmd.exe"));
    shells.push(ShellProfile::new("cmd", cmd, &[]));
    shells
}

fn unix_shells(env: &ShellEnv) -> Vec<ShellProfile> {
    let platform = Platform::Unix;
    let mut shells: Vec<ShellProfile> = Vec::new();
    if let Some(login) = env.shell.as_ref().map(PathBuf::from)
        && login.is_absolute()
        && is_program(platform, &login)
        && let Some(id) = login
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    {
        shells.push(ShellProfile::new(&id, login, &[]));
    }
    for name in ["bash", "zsh", "sh"] {
        if shells.iter().any(|shell| shell.id == name) {
            continue;
        }
        if let Some(program) = on_path(platform, env, name) {
            shells.push(ShellProfile::new(name, program, &[]));
        }
    }
    if shells.is_empty() {
        shells.push(ShellProfile::new("sh", PathBuf::from("/bin/sh"), &[]));
    }
    shells
}

/// `name` in the first absolute `PATH` directory that has it; an empty or relative entry is
/// passed over.
pub(crate) fn on_path(platform: Platform, env: &ShellEnv, name: &str) -> Option<PathBuf> {
    let path = env.path.as_ref()?.to_string_lossy().into_owned();
    path.split(platform.path_separator())
        .map(Path::new)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(name))
        .find(|candidate| is_program(platform, candidate))
}

/// Whether `path` is a file this platform would run.
pub(crate) fn is_program(platform: Platform, path: &Path) -> bool {
    path.is_file() && (platform == Platform::Windows || is_executable(path))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    path.metadata()
        .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    true
}

#[cfg(test)]
#[path = "shells_tests.rs"]
mod tests;
