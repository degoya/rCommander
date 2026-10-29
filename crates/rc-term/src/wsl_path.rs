//! The folder a WSL shell starts in, from a panel's Windows path.
//!
//! A drive path is the distribution's mount of that drive (`C:\Users\x` is `/mnt/c/Users/x`), a
//! path in the distribution's own share (`\\wsl.localhost\<distro>\home\x`, `\\wsl$\...`) is the
//! Linux path behind it. Everything else - another distribution's share, a network share, a
//! relative path - has no path in the distribution: the shell starts in its home (`~`) and the
//! reason becomes a hint for the status line.
//!
//! Pure string work, so the rules are tested on every platform, not only on Windows.

use std::path::{Path, PathBuf};

use rc_vfs::parse_wsl_unc;
use serde::{Deserialize, Serialize};

use crate::Platform;

/// The argument of `wsl.exe --cd` that means the user's home.
const HOME: &str = "~";

/// Where a WSL shell starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WslCwd {
    /// An absolute Linux path.
    Path(String),
    /// The user's home, because the folder has no path in the distribution.
    Home(HomeReason),
}

impl WslCwd {
    /// The argument of `wsl.exe --cd`.
    #[must_use]
    pub fn as_arg(&self) -> &str {
        match self {
            Self::Path(path) => path,
            Self::Home(_) => HOME,
        }
    }

    /// The i18n key of the hint why the shell starts in its home; `None` when it starts in the
    /// folder asked for.
    #[must_use]
    pub fn notice_key(&self) -> Option<&'static str> {
        match self {
            Self::Path(_) => None,
            Self::Home(reason) => Some(reason.notice_key()),
        }
    }
}

/// Why a folder has no path in the distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HomeReason {
    /// The folder is in another distribution's share (or the target distribution is unknown).
    OtherDistro,
    /// A network share, which WSL does not mount.
    Network,
    /// Not an absolute Windows path: relative, drive-relative or not Unicode.
    NotAbsolute,
}

impl HomeReason {
    /// The i18n key of the hint.
    #[must_use]
    pub fn notice_key(self) -> &'static str {
        match self {
            Self::OtherDistro => "terminal.wsl.home_other_distro",
            Self::Network => "terminal.wsl.home_network",
            Self::NotAbsolute => "terminal.wsl.home_not_absolute",
        }
    }
}

/// The folder a shell in `distro` starts in for the Windows folder `path`.
///
/// `distro` is the distribution the shell runs (WSL's default spelled out where it is known);
/// a share path matches it ignoring case, as Windows does. `None` matches no share.
#[must_use]
pub fn windows_to_wsl_path(path: &Path, distro: Option<&str>) -> WslCwd {
    if let Some(inside) = parse_wsl_unc(path) {
        return match distro {
            Some(target) if target.eq_ignore_ascii_case(&inside.distro) => {
                WslCwd::Path(inside.linux_path)
            }
            _ => WslCwd::Home(HomeReason::OtherDistro),
        };
    }
    let Some(text) = path.to_str() else {
        return WslCwd::Home(HomeReason::NotAbsolute);
    };
    let text = text.replace('/', "\\");
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    if text.starts_with(r"\\")
        || text
            .get(..4)
            .is_some_and(|unc| unc.eq_ignore_ascii_case(r"UNC\"))
    {
        return WslCwd::Home(HomeReason::Network);
    }
    match drive_path(text) {
        Some(linux) => WslCwd::Path(linux),
        None => WslCwd::Home(HomeReason::NotAbsolute),
    }
}

/// `C:\a\b` as `/mnt/c/a/b`; `None` unless `text` starts with a drive and its root.
fn drive_path(text: &str) -> Option<String> {
    let mut chars = text.chars();
    let letter = chars.next().filter(char::is_ascii_alphabetic)?;
    if chars.next() != Some(':') {
        return None;
    }
    let rest = chars.as_str();
    if !rest.is_empty() && !rest.starts_with('\\') {
        // `C:foo` is relative to the drive's current folder, which a shell does not have.
        return None;
    }
    let mut linux = format!("/mnt/{}", letter.to_ascii_lowercase());
    for part in rest.split('\\').filter(|part| !part.is_empty()) {
        linux.push('/');
        linux.push_str(part);
    }
    Some(linux)
}

/// How a shell names its folders, for the way between its paths and a panel's (RC-040-04).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellPaths {
    /// A shell of this machine: Windows paths on Windows, Unix paths elsewhere.
    Native(Platform),
    /// A shell in a WSL distribution, by its name; `None` when the name is not known.
    Wsl(Option<String>),
}

/// The panel folder of `reported`, a folder as the shell names it (OSC 7 path or OSC 9;9);
/// `None` when it has none in a panel.
///
/// Windows: `/C:/x` (OSC 7), `C:\x` and `C:/x` are `C:\x`, `\\server\share\x` stays. WSL:
/// `/mnt/c/x` is `C:\x`, every other path is in the distribution's share,
/// `\\wsl.localhost\<distro>\...`. Unix: an absolute path as it is.
#[must_use]
pub fn shell_to_panel_path(reported: &str, paths: &ShellPaths) -> Option<PathBuf> {
    match paths {
        ShellPaths::Native(Platform::Unix) => {
            reported.starts_with('/').then(|| PathBuf::from(reported))
        }
        ShellPaths::Native(Platform::Windows) => {
            let text = reported.replace('/', "\\");
            let text = match text.strip_prefix('\\') {
                // `/C:/x` of an OSC 7 URL.
                Some(rest) if rest.get(1..2) == Some(":") => rest.to_owned(),
                _ => text,
            };
            if text.starts_with(r"\\") {
                let unc = text.trim_end_matches('\\');
                return (unc.len() > 2).then(|| PathBuf::from(unc));
            }
            let letter = rc_vfs::drive_letter(&text)?;
            let rest = &text[2..];
            if !rest.is_empty() && !rest.starts_with('\\') {
                return None;
            }
            Some(drive_folder(letter, rest.split('\\')))
        }
        ShellPaths::Wsl(distro) => {
            if !reported.starts_with('/') {
                return None;
            }
            let mut parts = reported.split('/').filter(|part| !part.is_empty());
            let mut probe = parts.clone();
            if probe.next() == Some("mnt")
                && let Some(drive) = probe.next()
                && let [letter] = drive.as_bytes()
                && letter.is_ascii_alphabetic()
            {
                return Some(drive_folder(char::from(*letter), probe));
            }
            let mut share = format!(r"\\wsl.localhost\{}", distro.as_deref()?);
            for part in parts.by_ref() {
                share.push('\\');
                share.push_str(part);
            }
            Some(PathBuf::from(share))
        }
    }
}

/// `X:\a\b` from a drive letter and the folder names below its root.
fn drive_folder<'a>(letter: char, parts: impl Iterator<Item = &'a str>) -> PathBuf {
    let mut folder = format!("{}:\\", letter.to_ascii_uppercase());
    let names: Vec<&str> = parts.filter(|part| !part.is_empty()).collect();
    folder.push_str(&names.join("\\"));
    PathBuf::from(folder)
}

/// The panel folder `folder` as the shell names it, for its `cd`; `None` when the shell has no
/// path for it (a WSL shell and another distribution's share, a network share) or the name is
/// not Unicode.
#[must_use]
pub fn panel_to_shell_path(folder: &Path, paths: &ShellPaths) -> Option<String> {
    match paths {
        ShellPaths::Native(_) => folder.to_str().map(str::to_owned),
        ShellPaths::Wsl(distro) => match windows_to_wsl_path(folder, distro.as_deref()) {
            WslCwd::Path(path) => Some(path),
            WslCwd::Home(_) => None,
        },
    }
}

/// Whether a shell that names its folders as `paths` belongs in the panel folder `folder`: a
/// shell of this machine anywhere but in a WSL distribution's share, a WSL shell only in its own
/// distribution's share (in any distribution's when its name is not known).
#[must_use]
pub fn shell_fits(paths: &ShellPaths, folder: &Path) -> bool {
    let share = parse_wsl_unc(folder);
    match paths {
        ShellPaths::Native(_) => share.is_none(),
        ShellPaths::Wsl(distro) => share.is_some_and(|inside| {
            distro
                .as_deref()
                .is_none_or(|name| name.eq_ignore_ascii_case(&inside.distro))
        }),
    }
}

#[cfg(test)]
#[path = "wsl_path_tests.rs"]
mod tests;
