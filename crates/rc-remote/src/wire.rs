//! Paths on the wire, alike for FTP and SFTP (RC-080-48): a backend path spelled as the absolute
//! `/`-separated path a server takes, the wire path of a directory's child, and the home a
//! server names read back into a backend path.

use std::path::{Component, Path, PathBuf};

use rc_vfs::{Result, VfsError};

/// What one protocol says when it refuses a path, for [`absolute`].
pub(crate) struct Refusals {
    /// A relative path.
    pub relative: &'static str,
    /// A drive or share prefix.
    pub prefix: &'static str,
    /// A `..` component.
    pub parent: &'static str,
}

/// The wire path (`/a/b`) of a backend path: absolute and `/`-separated whatever the local
/// platform writes, `.` dropped.
///
/// Relative paths, `..`, Windows prefixes and names that are not Unicode are invalid;
/// `check_name` refuses each name the protocol cannot carry besides.
pub(crate) fn absolute(
    path: &Path,
    refusals: &Refusals,
    check_name: impl Fn(&str) -> Result<()>,
) -> Result<String> {
    let invalid = |reason| VfsError::InvalidPath {
        path: path.to_path_buf(),
        reason,
    };
    if !path.has_root() {
        return Err(invalid(refusals.relative));
    }
    let mut out = String::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Prefix(_) => return Err(invalid(refusals.prefix)),
            Component::ParentDir => return Err(invalid(refusals.parent)),
            Component::Normal(name) => {
                let name = name
                    .to_str()
                    .ok_or_else(|| invalid("the name is not Unicode"))?;
                check_name(name)?;
                out.push('/');
                out.push_str(name);
            }
        }
    }
    if out.is_empty() {
        out.push('/');
    }
    Ok(out)
}

/// The wire path of the child `name` of the wire directory `dir`.
pub(crate) fn child(dir: &str, name: &str) -> String {
    format!("{}/{name}", dir.trim_end_matches('/'))
}

/// The backend path of an absolute path the server named; `None` for one that is relative or
/// holds NUL, `\`, `.` or `..` as a name.
pub(crate) fn from_wire(wire: &str) -> Option<PathBuf> {
    let rest = wire.strip_prefix('/')?;
    let mut path = PathBuf::from("/");
    for name in rest.split('/').filter(|name| !name.is_empty()) {
        if matches!(name, "." | "..") || name.contains(['\0', '\\']) {
            return None;
        }
        path.push(name);
    }
    Some(path)
}

/// The home directory a server named (SFTP's `realpath(".")`, FTP's `PWD`), where a panel
/// starts: a plain absolute path by [`from_wire`] without a control character, `/` for anything
/// else or nothing. The server is not trusted to name one: an odd answer is not followed.
pub(crate) fn home(named: Option<&str>) -> PathBuf {
    named
        .filter(|named| !named.chars().any(char::is_control))
        .and_then(from_wire)
        .unwrap_or_else(|| PathBuf::from("/"))
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
