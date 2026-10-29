//! Which key file `secrets.import_key` reads (RC-050-06): a file on this computer, by the text of
//! its path, before anything is opened.
//!
//! A key path may come from a file the person did not write - an `IdentityFile` of an imported
//! ssh_config - and opening `\\server\share\key` on Windows sends the person's credentials to
//! that server. So a key is read only from a drive path (`C:\...`), an absolute path (`/...`,
//! `\...`), or the share of a WSL distribution (`\\wsl.localhost\<distro>\...`, `\\wsl$\...`),
//! which is this computer too. Any other share, a device or verbatim path (`\\?\`, `\\.\`,
//! `\??\`), a URL, a relative path and a control character are refused. A mapped network drive
//! looks like any drive and is read.

use rc_vfs::drive_letter;

/// Whether `path` names a key file on this computer.
#[must_use]
pub fn local_key_path(path: &str) -> bool {
    if path.chars().any(char::is_control) {
        return false;
    }
    let separator = |c: char| c == '\\' || c == '/';
    let mut chars = path.chars();
    match (chars.next(), chars.next(), chars.next()) {
        // Two separators: a share or a device path; only a WSL distribution's share is local.
        (Some(first), Some(second), _) if separator(first) && separator(second) => {
            wsl_share(&path[2..])
        }
        // `\??\` hands the rest to the NT object manager unparsed.
        (Some(first), Some('?'), _) if separator(first) => false,
        (Some(first), _, _) if separator(first) => true,
        (Some(_), Some(':'), Some(third)) => drive_letter(path).is_some() && separator(third),
        _ => false,
    }
}

/// Whether `rest`, the text after the leading `\\`, is `wsl.localhost\<distro>\...` or
/// `wsl$\<distro>\...`, without a `..` that could climb out of it.
fn wsl_share(rest: &str) -> bool {
    let mut parts = rest.split(['\\', '/']);
    let server = parts.next().unwrap_or_default();
    let distro = parts.next().unwrap_or_default();
    (server.eq_ignore_ascii_case("wsl.localhost") || server.eq_ignore_ascii_case("wsl$"))
        && !distro.is_empty()
        && !distro.contains(['?', '*', ':'])
        && rest.split(['\\', '/']).all(|part| part != "..")
        && parts.next().is_some_and(|first| !first.is_empty())
}

#[cfg(test)]
#[path = "key_path_tests.rs"]
mod tests;
