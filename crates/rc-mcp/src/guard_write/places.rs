//! Which places the write guard protects (`guard_write.rs`): the lists of names and folder
//! chains, this installation's data and program folders, and the judgement of one path by them.

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

use rc_db::Db;

use crate::{
    guard::{as_windows_opens, lowered},
    instance::INSTANCE_FILE,
};

/// Folder chains whose files start with the session, in lower case: Windows' Startup folders
/// (`%APPDATA%\Microsoft\Windows\...` and `%ProgramData%\...`) and the XDG autostart folder.
const AUTOSTART: &[&[&str]] = &[
    &["start menu", "programs", "startup"],
    &[".config", "autostart"],
    &[".config", "systemd", "user"],
];

/// Folder chains, in lower case, whose files a tool runs or obeys on its own, with the reason:
/// Git runs its hooks, fish its start-up file, an editor its tasks.
const RUN_BY_TOOLS: &[(&[&str], &str)] = &[
    (&[".git", "hooks"], "Git's hooks"),
    // A repository's and the XDG configuration name a pager, an editor and `core.fsmonitor`.
    (&[".git", "config"], "Git's configuration"),
    (&[".config", "git", "config"], "Git's configuration"),
    (&["fish", "config.fish"], "a shell's start-up file"),
    (&["fish", "conf.d"], "a shell's start-up file"),
    (&["fish", "functions"], "a shell's start-up file"),
    (&[".vscode", "tasks.json"], "an editor's tasks"),
    (&[".vscode", "settings.json"], "an editor's settings"),
    (&[".cargo", "config"], "Cargo's configuration"),
    (&[".cargo", "config.toml"], "Cargo's configuration"),
    // Programs found before the system's on the PATH, launchers, the session's environment.
    (&[".local", "bin"], "a folder on the PATH"),
    (&[".local", "share", "applications"], "a desktop launcher"),
    (&[".config", "environment.d"], "the session's environment"),
    (&["plasma-workspace", "env"], "the session's environment"),
];

/// File names a tool reads its programs from wherever they are: Git's configuration names an
/// editor, a pager and hooks it starts.
const TOOL_CONFIG: &[&str] = &[".gitconfig"];

/// The portable mode's marker file, in lower case.
const PORTABLE_MARKER: &str = rc_update::portable_mode::MARKER;

/// Shells' start-up files, run by the next shell or login; PowerShell's profiles end in
/// `profile.ps1`.
const SHELL_STARTUP: &[&str] = &[
    ".bash_aliases",
    ".bash_login",
    ".bash_logout",
    ".bash_profile",
    ".bashrc",
    ".cshrc",
    ".login",
    ".profile",
    ".pam_environment",
    ".tcshrc",
    ".xinitrc",
    ".xprofile",
    ".xsessionrc",
    ".zlogin",
    ".zlogout",
    ".zprofile",
    ".zshenv",
    ".zshrc",
];

/// Why MCP must not write at `path`, or `None` when it may; judged by its name and folders
/// against the default data folder.
#[must_use]
pub fn protected_target(path: &Path) -> Option<&'static str> {
    judge_all(path, default_data_dirs(), default_program_dirs())
}

/// [`judge`] of the path as given and as Windows opens it, the program's folders before it.
pub(super) fn judge_all(
    path: &Path,
    data_dirs: &[PathBuf],
    program_dirs: &[PathBuf],
) -> Option<&'static str> {
    let judged = |path: &Path| {
        if below_any(&lowered(path), program_dirs) {
            return Some("rCommander's program folder");
        }
        judge(path, data_dirs)
    };
    judged(path).or_else(|| judged(&as_windows_opens(path)?))
}

/// Whether the names `parts` are one of `dirs` or lie below one.
fn below_any(parts: &[String], dirs: &[PathBuf]) -> bool {
    dirs.iter()
        .map(|dir| lowered(dir))
        .any(|dir| !dir.is_empty() && parts.starts_with(&dir))
}

/// Why the folder at `path` may not be moved or renamed away (RC-110-19): it holds one of this
/// installation's program or data folders, judged as given and as Windows opens it. The places
/// known by name alone are found by the entries below the folder (`WriteGuard::refuse_taking`).
pub(super) fn judge_holding(
    path: &Path,
    data_dirs: &[PathBuf],
    program_dirs: &[PathBuf],
) -> Option<&'static str> {
    let judged = |path: &Path| {
        let parts = lowered(path);
        if above_any(&parts, program_dirs) {
            return Some("rCommander's program folder");
        }
        above_any(&parts, data_dirs).then_some("rCommander's data folder")
    };
    judged(path).or_else(|| judged(&as_windows_opens(path)?))
}

/// Whether one of `dirs` is the folder named `parts` or lies below it; a root holds them all.
fn above_any(parts: &[String], dirs: &[PathBuf]) -> bool {
    dirs.iter()
        .map(|dir| lowered(dir))
        .any(|dir| !dir.is_empty() && dir.starts_with(parts))
}

/// [`protected_target`] with the data folders given.
pub(super) fn judge(path: &Path, data_dirs: &[PathBuf]) -> Option<&'static str> {
    let parts = lowered(path);
    if below_any(&parts, data_dirs) {
        return Some("rCommander's data folder");
    }
    // The marker's content does not matter, so any file of that name is one (RC-110-19).
    if parts.iter().any(|name| name == PORTABLE_MARKER) {
        return Some("the marker of rCommander's portable mode");
    }
    if parts
        .iter()
        .any(|name| rc_db::is_database_file_name(name) || name == INSTANCE_FILE)
    {
        return Some("one of rCommander's own files");
    }
    if parts.iter().any(|name| name == ".ssh") {
        return Some("an SSH folder");
    }
    // The folders an update keeps beside the program (RC-100-06): the staging, the previous
    // version, the trash and a rolled-back version; the data folder's `update/` is covered above.
    if parts.iter().any(|name| {
        name == ".previous"
            || name == ".trash"
            || name.starts_with(".update-")
            || name.starts_with(".failed-")
    }) {
        return Some("an update's own folder");
    }
    let holds = |chain: &[&str]| {
        parts.windows(chain.len()).any(|window| {
            window
                .iter()
                .zip(chain.iter())
                .all(|(name, want)| name == want)
        })
    };
    if AUTOSTART.iter().any(|chain| holds(chain)) {
        return Some("an autostart folder");
    }
    if let Some((_, reason)) = RUN_BY_TOOLS.iter().find(|(chain, _)| holds(chain)) {
        return Some(*reason);
    }
    if parts
        .iter()
        .any(|name| TOOL_CONFIG.contains(&name.as_str()))
    {
        return Some("Git's configuration");
    }
    let name = parts.last()?;
    if SHELL_STARTUP.contains(&name.as_str()) || name.ends_with("profile.ps1") {
        return Some("a shell's start-up file");
    }
    None
}

/// The default data folder, as the platform names it and with links resolved; empty when the
/// platform has no data directory.
pub(super) fn default_data_dirs() -> &'static [PathBuf] {
    static DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    DIRS.get_or_init(|| {
        Db::default_path()
            .ok()
            .and_then(|db| db.parent().map(Path::to_path_buf))
            .map(with_real)
            .unwrap_or_default()
    })
}

/// The folder of this process's program, see [`program_dirs_of`]; empty when the platform does
/// not name the program.
fn default_program_dirs() -> &'static [PathBuf] {
    static DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    DIRS.get_or_init(|| {
        std::env::current_exe()
            .map(|executable| program_dirs_of(&executable))
            .unwrap_or_default()
    })
}

/// The folder of the program at `executable` under every name it has: as given, where that
/// folder really is, and where the program really is when its own name is a link.
pub(super) fn program_dirs_of(executable: &Path) -> Vec<PathBuf> {
    let real = std::fs::canonicalize(executable).ok();
    let mut dirs: Vec<PathBuf> = Vec::new();
    for folder in [executable.parent(), real.as_deref().and_then(Path::parent)] {
        for dir in folder
            .map(Path::to_path_buf)
            .map(with_real)
            .unwrap_or_default()
        {
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
    }
    dirs
}

/// `dir` and, when it differs and exists, where it really is.
pub(super) fn with_real(dir: PathBuf) -> Vec<PathBuf> {
    let real = std::fs::canonicalize(&dir).ok();
    let mut dirs = vec![dir];
    dirs.extend(real);
    dirs
}

/// The program's folders a server's context starts with: those of this process.
pub(crate) fn program_dirs() -> Vec<PathBuf> {
    default_program_dirs().to_vec()
}
