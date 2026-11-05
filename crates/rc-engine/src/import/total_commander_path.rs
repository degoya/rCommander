//! Where Total Commander keeps `wincmd.ini` and `wcx_ftp.ini` (RC-050-09).
//!
//! `wincmd.ini` is at `%APPDATA%\GHISLER\wincmd.ini`, else where Total Commander's registry key
//! says (`IniFileName`, Windows only); `wcx_ftp.ini` where `wincmd.ini` names it (`[FTP]
//! FtpIniName`), else beside `wincmd.ini`, else where the registry says (`FtpIniName`). A file
//! another file names is read only when its name is `wcx_ftp.ini`, so a `wincmd.ini` cannot point
//! the import at any other file. `%variables%` come from the environment, `%COMMANDER_PATH%` from
//! the registry's `InstallDir` when the environment has none - only the variables Total Commander
//! itself puts into paths ([`KNOWN_VARIABLES`], RC-080-35). Any other stays as written, as one
//! that is not set does in Total Commander (RC-080-71): a crafted `wincmd.ini` (`cd
//! C:\%AWS_SECRET_ACCESS_KEY%`) cannot copy a secret of the environment into the report's paths,
//! which MCP reads too. One of the list set to nothing is nothing.

use std::path::{Path, PathBuf};

use super::total_commander::Lookup;

/// Total Commander's main settings file.
pub const WINCMD_INI: &str = "wincmd.ini";

/// Its FTP connections.
pub const WCX_FTP_INI: &str = "wcx_ftp.ini";

/// What Total Commander's registry key says; all `None` off Windows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registry {
    pub ini_file: Option<String>,
    pub ftp_ini: Option<String>,
    pub install_dir: Option<String>,
}

#[cfg(windows)]
mod windows;

/// Total Commander's registry values for this user (or the machine).
#[must_use]
pub fn registry() -> Registry {
    #[cfg(windows)]
    {
        windows::read()
    }
    #[cfg(not(windows))]
    {
        Registry::default()
    }
}

/// The `%variables%` an import resolves, compared ignoring ASCII case: those Total Commander
/// itself uses in paths. No switch widens it - it is a security boundary.
pub const KNOWN_VARIABLES: &[&str] = &[
    "COMMANDER_PATH",
    "COMMANDER_INI",
    "APPDATA",
    "LOCALAPPDATA",
    "USERPROFILE",
    "USERNAME",
    "HOMEDRIVE",
    "HOMEPATH",
    "ProgramFiles",
    "ProgramFiles(x86)",
    "ProgramW6432",
    "ProgramData",
    "PUBLIC",
    "SystemDrive",
    "SystemRoot",
    "windir",
    "TEMP",
    "TMP",
];

/// `%name%` from the environment, `COMMANDER_PATH` from `install_dir` when the environment has
/// none; `None` for a name not in [`KNOWN_VARIABLES`].
#[must_use]
pub fn environment(name: &str, install_dir: Option<&str>) -> Option<String> {
    resolve_known(name, install_dir, &|name| std::env::var(name).ok())
}

/// [`environment`] over `env` instead of the process environment.
fn resolve_known(
    name: &str,
    install_dir: Option<&str>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    if !is_known(name) {
        return None;
    }
    env(name).or_else(|| {
        install_dir
            .filter(|dir| !dir.is_empty() && name.eq_ignore_ascii_case("COMMANDER_PATH"))
            .map(str::to_owned)
    })
}

/// Whether `name` is one of [`KNOWN_VARIABLES`], ignoring ASCII case.
fn is_known(name: &str) -> bool {
    KNOWN_VARIABLES
        .iter()
        .any(|known| known.eq_ignore_ascii_case(name))
}

/// `text` with every `%name%` that `lookup` knows replaced by its value.
///
/// As Total Commander does (`history.txt`, 9.21 RC2, 29.07.18: "Distinguish between empty
/// environment variables and non-existing environment variables. Replace %variable% with empty
/// string in the first case"): a variable that is not set stays as written, `%` and all, one set
/// to nothing becomes nothing, and the names after an unknown one are still replaced (8.51b4); a
/// `%` that opens no name (`C:\100% done`) is text. Total Commander reads any variable; `lookup`
/// answers only [`KNOWN_VARIABLES`] (RC-080-35), so any other name stays as written too - never
/// read from the environment, it cannot carry a secret into the report.
#[must_use]
pub fn expand(text: &str, lookup: Lookup<'_>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        // Parentheses for `%ProgramFiles(x86)%`.
        let allowed = |c: char| c.is_alphanumeric() || matches!(c, '_' | '(' | ')');
        let name = after
            .find('%')
            .map(|end| &after[..end])
            .filter(|name| !name.is_empty() && name.chars().all(allowed));
        let Some(name) = name else {
            // No name follows: the `%` is text, and the next `%` may open one.
            out.push('%');
            rest = after;
            continue;
        };
        match lookup(name) {
            Some(value) => out.push_str(&value),
            None => {
                out.push('%');
                out.push_str(name);
                out.push('%');
            }
        }
        rest = &after[name.len() + 1..];
    }
    out.push_str(rest);
    out
}

/// `path` without the `\\?\` prefix of a drive or a share (`\\?\C:\x`, `\\?\UNC\host\share`):
/// the same folder, and Total Commander puts the prefix in front of a long path itself, as
/// rCommander's local backend does (author, ghisler.ch `viewtopic.php?p=357612`). A volume
/// (`\\?\Volume{...}`, which `cd` takes: `history.txt`, 10.11.21) and `\\.\` keep what they are.
pub(super) fn without_verbatim_prefix(path: &str) -> String {
    let Some(rest) = path.strip_prefix("\\\\?\\") else {
        return path.to_owned();
    };
    if let Some(share) = rest
        .get(..4)
        .filter(|unc| unc.eq_ignore_ascii_case("UNC\\"))
        .map(|_| &rest[4..])
    {
        return format!("\\\\{share}");
    }
    if rc_vfs::drive_letter(rest).is_some() {
        return rest.to_owned();
    }
    path.to_owned()
}

/// The last part of `path`, whichever separator it uses.
fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Whether `path` names a file called `name`, ignoring ASCII case.
#[must_use]
pub fn is_named(path: &Path, name: &str) -> bool {
    file_name(&path.to_string_lossy()).eq_ignore_ascii_case(name)
}

/// `raw` from a file or the registry as a path: expanded, and when relative, below `base`. On
/// Windows a path from the root of a drive (`\totalcmd\wincmd.ini`) takes `base`'s drive, and
/// without `base` stays as it is: the current drive's.
fn resolve(raw: &str, base: Option<&Path>, lookup: Lookup<'_>) -> Option<PathBuf> {
    let expanded = expand(raw.trim().trim_matches('"'), lookup);
    if expanded.is_empty() {
        return None;
    }
    let path = PathBuf::from(&expanded);
    let rooted = path.is_absolute() || expanded.starts_with("\\\\") || expanded.contains(":\\");
    if rooted {
        return Some(path);
    }
    let relative = expanded
        .strip_prefix(".\\")
        .or_else(|| expanded.strip_prefix("./"))
        .unwrap_or(&expanded);
    match base {
        Some(base) => Some(base.join(relative)),
        None => path.has_root().then_some(path),
    }
}

/// Where `wincmd.ini` may be, most likely first.
#[must_use]
pub fn wincmd_candidates(lookup: Lookup<'_>, registry: &Registry) -> Vec<PathBuf> {
    let install = registry.install_dir.as_deref().map(Path::new);
    let mut found: Vec<PathBuf> = lookup("APPDATA")
        .map(|appdata| PathBuf::from(appdata).join("GHISLER").join(WINCMD_INI))
        .into_iter()
        .collect();
    found.extend(
        registry
            .ini_file
            .as_deref()
            .and_then(|raw| resolve(raw, install, lookup)),
    );
    found
}

/// Where `wcx_ftp.ini` may be, most likely first: only files of that name.
#[must_use]
pub fn ftp_candidates(
    wincmd: Option<&Path>,
    ftp_ini_name: Option<&str>,
    lookup: Lookup<'_>,
    registry: &Registry,
) -> Vec<PathBuf> {
    let beside = wincmd.and_then(Path::parent);
    let install = registry.install_dir.as_deref().map(Path::new);
    let mut found = Vec::new();
    found.extend(ftp_ini_name.and_then(|raw| resolve(raw, beside, lookup)));
    found.extend(beside.map(|dir| dir.join(WCX_FTP_INI)));
    found.extend(
        registry
            .ftp_ini
            .as_deref()
            .and_then(|raw| resolve(raw, install, lookup)),
    );
    found.retain(|path| is_named(path, WCX_FTP_INI));
    found.dedup();
    found
}

#[cfg(test)]
#[path = "total_commander_path_tests.rs"]
mod tests;
