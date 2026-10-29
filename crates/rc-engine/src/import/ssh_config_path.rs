//! Which file the ssh_config import may read (RC-050-06): `<home>/.ssh/config` or a file below
//! `<home>/.ssh/config.d/`, nothing else - for the local user's home, or a WSL distribution's
//! home on its share (`\\wsl.localhost\<distro>\home\<user>`, `...\root`, also `\\wsl$`).
//!
//! The rule is checked on the canonical path, so neither `..` nor a link leads out of it. Any
//! other file would hand its content to the caller - an MCP agent among them - as hosts and
//! warnings. What the configuration itself includes may lie elsewhere, as in OpenSSH; the
//! parser's key guard (`ssh_config::guard`) still keeps every key out.

use std::path::{Component, Path, PathBuf, Prefix};

use super::ssh_config::SshConfigContext;

/// The home whose configuration `path` is, when it is one of the files the import reads.
#[must_use]
pub fn config_home(path: &Path, local_home: Option<&Path>) -> Option<PathBuf> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| component == Component::ParentDir)
    {
        return None;
    }
    local_home
        .map(Path::to_path_buf)
        .into_iter()
        .chain(wsl_home(path))
        .find(|home| is_config_of(path, home))
}

/// Whether `path` is `<home>/.ssh/config` or lies below `<home>/.ssh/config.d/`.
fn is_config_of(path: &Path, home: &Path) -> bool {
    let ssh = home.join(".ssh");
    let drop_in = ssh.join("config.d");
    path == ssh.join("config") || (path.starts_with(&drop_in) && path != drop_in)
}

/// The home a path on a WSL share lies in: `/home/<user>` or `/root` of the distribution.
fn wsl_home(path: &Path) -> Option<PathBuf> {
    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return None;
    };
    let (Prefix::UNC(server, _) | Prefix::VerbatimUNC(server, _)) = prefix.kind() else {
        return None;
    };
    if !server.eq_ignore_ascii_case("wsl.localhost") && !server.eq_ignore_ascii_case("wsl$") {
        return None;
    }
    let rest: Vec<Component<'_>> = components.take(3).collect();
    let length = match rest.as_slice() {
        [
            Component::RootDir,
            Component::Normal(top),
            Component::Normal(_),
        ] if top.eq_ignore_ascii_case("home") => 4,
        [Component::RootDir, Component::Normal(top), ..] if top.eq_ignore_ascii_case("root") => 3,
        _ => return None,
    };
    Some(path.components().take(length).collect())
}

/// `home` as a WSL share without the verbatim prefix `fs::canonicalize` gives it on Windows
/// (`\\?\UNC\wsl$\<distro>\...` becomes `\\wsl$\<distro>\...`), so a key below it is shown,
/// and taken into the vault, as the distribution's share it is - `secrets::local_key_path`
/// refuses every verbatim path. Any other path stays as it is, and so does one a plain path
/// would read differently (a `/`, or a name ending in a dot or a space).
#[must_use]
pub fn plain_wsl(home: &Path) -> PathBuf {
    let plain = home.to_str().and_then(|text| {
        let rest = text.strip_prefix(r"\\?\UNC\")?;
        let literal = rest.contains('/') || rest.split('\\').any(|part| part.ends_with(['.', ' ']));
        (!literal && rc_vfs::parse_wsl_unc(home).is_some())
            .then(|| PathBuf::from(format!(r"\\{rest}")))
    });
    plain.unwrap_or_else(|| home.to_path_buf())
}

/// The context to read a configuration of `home` in: its `.ssh` directory, and the local user
/// when it is the local home, else the home directory's name.
#[must_use]
pub fn context_for(home: &Path, local_home: Option<&Path>, local_user: &str) -> SshConfigContext {
    let user = if local_home == Some(home) {
        local_user.to_owned()
    } else {
        home.file_name().map_or_else(
            || local_user.to_owned(),
            |name| name.to_string_lossy().into_owned(),
        )
    };
    SshConfigContext::new(home, user)
}

#[cfg(test)]
#[path = "ssh_config_path_tests.rs"]
mod tests;
