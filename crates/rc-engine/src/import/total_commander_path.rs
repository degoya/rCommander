//! Where Total Commander keeps `wincmd.ini` and `wcx_ftp.ini` (RC-050-09).
//!
//! `wincmd.ini` is at `%APPDATA%\GHISLER\wincmd.ini`, else where Total Commander's registry key
//! says (`IniFileName`, Windows only); `wcx_ftp.ini` where `wincmd.ini` names it (`[FTP]
//! FtpIniName`), else beside `wincmd.ini`, else where the registry says (`FtpIniName`). A file
//! another file names is read only when its name is `wcx_ftp.ini`, so a `wincmd.ini` cannot point
//! the import at any other file. `%variables%` come from the environment, `%COMMANDER_PATH%` from
//! the registry's `InstallDir` when the environment has none - only the variables Total Commander
//! itself puts into paths ([`KNOWN_VARIABLES`], RC-080-35). Any other stays unresolved, and the
//! item naming it is skipped as `unknown_variable`: a crafted `wincmd.ini` (`cd
//! C:\%AWS_SECRET_ACCESS_KEY%`) cannot copy a secret of the environment into the report's paths,
//! which MCP reads too.

use std::path::{Path, PathBuf};

use super::total_commander::{Lookup, expand};

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
    if !KNOWN_VARIABLES
        .iter()
        .any(|known| known.eq_ignore_ascii_case(name))
    {
        return None;
    }
    env(name)
        .or_else(|| {
            install_dir
                .filter(|_| name.eq_ignore_ascii_case("COMMANDER_PATH"))
                .map(str::to_owned)
        })
        .filter(|value| !value.is_empty())
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

/// `raw` from a file or the registry as a path: expanded, and when relative, below `base`.
fn resolve(raw: &str, base: Option<&Path>, lookup: Lookup<'_>) -> Option<PathBuf> {
    let expanded = expand(raw.trim().trim_matches('"'), lookup)?;
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
    base.map(|base| base.join(relative))
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
