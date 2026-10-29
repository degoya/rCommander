//! A file name the person typed (the "rename to" answer of a conflict): untrusted input, so it
//! must be one plain name that every backend - Windows first - can hold, and nothing that
//! leaves the target directory. The same rules hold for every name a copy gives a local folder
//! on Windows ([`check_target`]): what a server listed or an archive stored included. The rule
//! itself is `rc-rename`'s, the one every crate holds a name to (RC-080-43).

use std::path::{Component, Path};

use rc_rename::{InvalidName, check_plain};
use rc_vfs::BackendId;

use crate::OpsError;

/// Accepts `name` as one plain file name - the one name rule, [`rc_rename::check_plain`].
///
/// # Errors
///
/// `invalid_name` with the reason: empty, `.` or `..`, too long (255 bytes), a separator or
/// another character Windows refuses (`:` an NTFS stream among them), a control character, a
/// character that turns the reading direction, a trailing dot or space, a device name.
pub fn check_name(name: &str) -> Result<(), OpsError> {
    match check_plain(name) {
        None => Ok(()),
        Some(problem) => Err(OpsError::InvalidName {
            name: name.to_owned(),
            reason: reason(problem),
        }),
    }
}

/// The reason `invalid_name` names for `problem`.
fn reason(problem: InvalidName) -> &'static str {
    match problem {
        InvalidName::Empty => "empty",
        InvalidName::DotName => "a relative path",
        InvalidName::NameTooLong | InvalidName::PathTooLong => "too long",
        InvalidName::Separator | InvalidName::ReservedChar { .. } => {
            "a separator or a character Windows refuses"
        }
        InvalidName::ControlChar => "a control character",
        InvalidName::TurnsDirection => "a character that turns the reading direction",
        InvalidName::TrailingDotOrSpace => "ends with a dot or a space",
        InvalidName::ReservedName => "a Windows device name",
        InvalidName::ParentDir
        | InvalidName::AbsolutePath
        | InvalidName::DrivePath
        | InvalidName::UncPath
        | InvalidName::EmptyPart => "a path that is not plain names",
    }
}

/// Whether the names written to `backend` must keep Windows' rules: a local folder on Windows.
#[must_use]
pub fn keeps_windows_rules(backend: &BackendId) -> bool {
    cfg!(windows) && *backend == BackendId::Local
}

/// Accepts `rel`, the path an entry gets below the folder a copy writes into, part by part as
/// [`check_name`] judges a typed name - when `strict`: the target keeps Windows' rules
/// ([`keeps_windows_rules`]), or the names come out of an archive, judged as unpacking judges
/// them. A name a server listed or an archive stored may be `a:b`, `con` or `report.`, which
/// Windows turns into a stream, a device or another name. Otherwise any name the source could
/// hold is kept.
///
/// # Errors
///
/// `invalid_name` for the first part refused, or a part that is no plain UTF-8 name.
pub fn check_target(rel: &Path, strict: bool) -> Result<(), OpsError> {
    if !strict {
        return Ok(());
    }
    for component in rel.components() {
        match component {
            Component::Normal(part) => match part.to_str() {
                Some(part) => check_name(part)?,
                None => {
                    return Err(OpsError::InvalidName {
                        name: part.to_string_lossy().into_owned(),
                        reason: "a name that is not UTF-8",
                    });
                }
            },
            _ => {
                return Err(OpsError::InvalidName {
                    name: rel.display().to_string(),
                    reason: "a path that is not plain names",
                });
            }
        }
    }
    Ok(())
}

/// Accepts `name`, as a listing reported it, as one entry of its directory: exactly one plain
/// path component, which a join cannot turn into a way out of the directory (`..`, `/abs`, `a/b`,
/// and on Windows `a\b`, `C:x` or a stream `a:b`). A server's listing is untrusted input.
///
/// # Errors
///
/// `invalid_name` with the reason.
pub(crate) fn check_listed(name: &str) -> Result<(), OpsError> {
    let mut components = Path::new(name).components();
    let plain = matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(only)), None) if only == name
    );
    let windows_special = cfg!(windows) && name.contains([':', '\\']);
    if plain && !windows_special {
        Ok(())
    } else {
        Err(OpsError::InvalidName {
            name: name.to_owned(),
            reason: "a listed name that is not one plain name",
        })
    }
}

#[cfg(test)]
#[path = "name_tests.rs"]
mod tests;
