//! Windows path forms: long paths past the 260-character limit, and WSL distributions as UNC
//! shares.
//!
//! Pure string work on purpose, so the rules are tested on Linux where the tests run, not only
//! on the platform they are for.

use std::path::{Path, PathBuf};

use crate::{Result, VfsError};

/// The UNC host Windows 11 and current Windows 10 serve WSL distributions under.
pub(crate) const WSL_HOST: &str = "wsl.localhost";
/// The older host name, still accepted by Windows.
pub(crate) const WSL_LEGACY_HOST: &str = "wsl$";

/// The form of `path` that Windows will not truncate at 260 characters. Unchanged on every
/// other platform, and for a path that cannot take the prefix (relative, or holding `.` or
/// `..`, which a verbatim path would pass to the file system as names).
///
/// `\\?\` turns the limit off for one call; the kernel takes such a path as it stands, so it
/// has to be fully qualified with backslashes only.
#[must_use]
pub fn long_path(path: &Path) -> PathBuf {
    if cfg!(windows) {
        verbatim(path).unwrap_or_else(|| path.to_path_buf())
    } else {
        path.to_path_buf()
    }
}

/// The `\\?\` form of an absolute Windows path, or `None` when there is none.
pub(crate) fn verbatim(path: &Path) -> Option<PathBuf> {
    let text = path.to_str()?;
    if text.starts_with(r"\\?\") || text.starts_with(r"\\.\") {
        return Some(path.to_path_buf());
    }
    let normalised = text.replace('/', "\\");
    if normalised
        .split('\\')
        .any(|component| component == "." || component == "..")
    {
        return None;
    }
    if drive_letter(&normalised).is_some() && normalised.as_bytes().get(2) == Some(&b'\\') {
        return Some(PathBuf::from(format!(r"\\?\{normalised}")));
    }
    // `\\server\share\...` becomes `\\?\UNC\server\share\...`.
    if let Some(rest) = normalised.strip_prefix(r"\\")
        && rest.split('\\').filter(|part| !part.is_empty()).count() >= 2
    {
        return Some(PathBuf::from(format!(r"\\?\UNC\{rest}")));
    }
    None
}

/// The Windows UNC path of `linux_path` inside the WSL distribution `distro`:
/// `wsl_unc_path("Ubuntu", "/home/me")` is `\\wsl.localhost\Ubuntu\home\me`.
///
/// The result is a plain local path on Windows, which [`crate::LocalFs`] lists like any share.
///
/// # Errors
///
/// [`VfsError::InvalidPath`] when `distro` is not a valid share name or `linux_path` is not
/// absolute or holds a backslash (which a UNC path cannot express).
pub fn wsl_unc_path(distro: &str, linux_path: &str) -> Result<PathBuf> {
    let invalid = |reason| VfsError::InvalidPath {
        path: PathBuf::from(format!("{distro}:{linux_path}")),
        reason,
    };
    if !valid_distro(distro) {
        return Err(invalid("not a WSL distribution name"));
    }
    if !linux_path.starts_with('/') {
        return Err(invalid("the Linux path is not absolute"));
    }
    if linux_path.contains('\\') {
        return Err(invalid("a backslash cannot be expressed in a UNC path"));
    }
    let rest = linux_path.trim_start_matches('/').replace('/', "\\");
    Ok(PathBuf::from(format!(r"\\{WSL_HOST}\{distro}\{rest}")))
}

/// A path inside a WSL distribution, split out of its UNC form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WslPath {
    /// The distribution, as the share name spells it.
    pub distro: String,
    /// The absolute Linux path, with `/` separators.
    pub linux_path: String,
}

/// Splits a WSL UNC path - `\\wsl.localhost\<distro>\...`, `\\wsl$\<distro>\...`, their `\\?\UNC\`
/// forms or the same with forward slashes - into distribution and Linux path. `None` for any
/// other path.
#[must_use]
pub fn parse_wsl_unc(path: &Path) -> Option<WslPath> {
    let text = path.to_str()?.replace('/', "\\");
    let rest = text
        .strip_prefix(r"\\?\UNC\")
        .or_else(|| text.strip_prefix(r"\\"))?;
    let (host, rest) = rest.split_once('\\')?;
    if !host.eq_ignore_ascii_case(WSL_HOST) && !host.eq_ignore_ascii_case(WSL_LEGACY_HOST) {
        return None;
    }
    let (distro, inner) = rest.split_once('\\').unwrap_or((rest, ""));
    if !valid_distro(distro) {
        return None;
    }
    let inner = inner.trim_end_matches('\\');
    Some(WslPath {
        distro: distro.to_owned(),
        linux_path: format!("/{}", inner.replace('\\', "/")),
    })
}

/// What a path reaches on another machine, from [`network_path`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkPath {
    /// A share, as `\\server\share`: of `\\server\share\...`, `//server/share`,
    /// `\\?\UNC\server\share\...` or `\\.\UNC\...`.
    Share(String),
    /// A device or NT path (`\\.\pipe\x`, `\\?\GLOBALROOT\...`, `\??\UNC\...`) or a UNC path
    /// without a share: Windows may still send it to a server.
    Other,
}

/// What `text` reaches on another machine when Windows opens it, or `None` for a path that
/// stays on this one: a drive path (also `\\?\C:\...`), a relative or root-relative path, a Unix
/// path, and a WSL distribution's share (`\\wsl.localhost\<distro>`, `\\wsl$\<distro>`), which
/// Windows serves itself.
///
/// Opening a share makes Windows sign in to its server, sending the person's NTLM hash; a path
/// a terminal's output or an agent names is judged here before anything opens it (RC-080-10).
/// Judged on the text alone, `/` read as `\`, so the rule is the same on every platform.
#[must_use]
pub fn network_path(text: &str) -> Option<NetworkPath> {
    let text = text.replace('/', "\\");
    let rest = text.trim_start_matches('\\');
    let lead = text.len() - rest.len();
    if lead == 1 && (rest == "??" || rest.starts_with(r"??\")) {
        return Some(NetworkPath::Other);
    }
    if lead < 2 || parse_wsl_unc(Path::new(&format!(r"\\{rest}"))).is_some() {
        return None;
    }
    let unc = match rest
        .strip_prefix(r"?\")
        .or_else(|| rest.strip_prefix(r".\"))
    {
        Some(device) if drive_root(device) => return None,
        Some(device) => match device.get(..4) {
            Some(prefix) if prefix.eq_ignore_ascii_case(r"UNC\") => &device[4..],
            _ => return Some(NetworkPath::Other),
        },
        None => rest,
    };
    let mut parts = unc.split('\\');
    match (parts.next(), parts.next()) {
        (Some(server), Some(share)) if !server.is_empty() && !share.is_empty() => {
            Some(NetworkPath::Share(format!(r"\\{server}\{share}")))
        }
        _ => Some(NetworkPath::Other),
    }
}

/// Whether `text` starts with a drive and its root: `C:` or `C:\...`.
fn drive_root(text: &str) -> bool {
    drive_letter(text).is_some() && text.as_bytes().get(2).is_none_or(|&byte| byte == b'\\')
}

/// The drive letter `text` starts with, as written: of `C:`, `c:\x`, `D:/`, also `C:x`; `None`
/// for `c`, `/C:` and anything else. What follows the colon - a separator, nothing, a name - is
/// the caller's to judge; the two characters before it are ASCII, so `text[2..]` is sound.
#[must_use]
pub fn drive_letter(text: &str) -> Option<char> {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(letter), Some(':')) if letter.is_ascii_alphabetic() => Some(letter),
        _ => None,
    }
}

/// A name split at its last dot into stem and extension (without the dot), as Total Commander
/// splits it: `notes.txt` is `("notes", "txt")`, `a.tar.gz` is `("a.tar", "gz")`. A dot that
/// starts the name (`.bashrc`) or ends it (`notes.`) splits nothing: `("notes.", "")`. A
/// folder's name is split like a file's.
#[must_use]
pub fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < name.len() => (&name[..dot], &name[dot + 1..]),
        _ => (name, ""),
    }
}

/// Whether `name` can be the share name of a distribution.
pub(crate) fn valid_distro(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.chars().any(|c| {
            c.is_control() || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
}

/// `path` without the `\\?\` prefix [`long_path`] puts on: `\\?\C:\x` is `C:\x`,
/// `\\?\UNC\server\share` is `\\server\share`. Unchanged on every other platform, for a path
/// without the prefix, and for one holding `.` or `..` (a verbatim path takes them as names, a
/// plain one does not). A path that only the prefix made valid - past 260 characters, a
/// reserved name like `COM1` - is shortened all the same: every call puts the prefix back
/// through [`long_path`] (RC-080-31).
#[must_use]
pub(crate) fn strip_verbatim(path: &Path) -> PathBuf {
    if cfg!(windows) {
        unverbatim(path).unwrap_or_else(|| path.to_path_buf())
    } else {
        path.to_path_buf()
    }
}

/// The plain form of a `\\?\` path, or `None` when it has none.
fn unverbatim(path: &Path) -> Option<PathBuf> {
    let rest = path.to_str()?.strip_prefix(r"\\?\")?;
    if rest
        .split('\\')
        .any(|component| component == "." || component == "..")
    {
        return None;
    }
    if let Some(share) = rest.strip_prefix(r"UNC\") {
        return Some(PathBuf::from(format!(r"\\{share}")));
    }
    let bytes = rest.as_bytes();
    (bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\')
        .then(|| PathBuf::from(rest))
}

#[cfg(test)]
#[path = "paths_tests.rs"]
mod tests;
