//! `connections.import_ssh_config`: the hosts of an OpenSSH client configuration into the
//! connection tree, or with `dry_run` only the preview of that (RC-050-06).
//!
//! Every caller, MCP included, gets the same file rule (`import::ssh_config_path`): only
//! `~/.ssh/config` or a file below `~/.ssh/config.d/`, by canonical path, and never a key
//! (`import::ssh_config::guard`). Under MCP an import never updates a connection that holds a
//! stored secret: a changed config would move the secret's endpoint (`commands/connections.rs`);
//! such a host is reported refused with `endpoint_change_needs_gui`.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use rc_db::connections::{ImportCandidate, ImportStatus, ImportTarget};
use rc_vfs::BackendId;

use crate::{
    Engine, EngineError, FolderId, Outcome, TreeChange, TreeItem,
    import::{
        ssh_config::{self, LoadError, McpScope, SshConfigImport},
        ssh_config_apply::{self, DEFAULT_FOLDER},
        ssh_config_path::{config_home, context_for, plain_wsl},
    },
    read_guard::{self, ReadGuard},
};

/// The local user the import reads for: the home `~` stands for, and the name for `%u`.
#[derive(Debug, Clone)]
pub(crate) struct LocalUser {
    pub(crate) home: Option<PathBuf>,
    pub(crate) name: String,
}

impl LocalUser {
    /// The user this process runs as.
    fn current() -> Self {
        Self {
            home: directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf()),
            name: std::env::var("USERNAME")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_default(),
        }
    }
}

impl Engine {
    /// `connections.import_ssh_config` for the user this process runs as.
    pub(crate) async fn import_ssh_config(
        &self,
        path: Option<&str>,
        dry_run: bool,
        hosts: Option<&[String]>,
        folder_id: Option<FolderId>,
    ) -> Result<Outcome, EngineError> {
        self.import_ssh_config_for(LocalUser::current(), path, dry_run, hosts, folder_id)
            .await
    }

    /// Reads `path` (`~/.ssh/config` when `None`) and imports its hosts - those `hosts` names,
    /// or all - into `folder_id`, or the root's [`DEFAULT_FOLDER`]. With `dry_run` nothing is
    /// written.
    pub(crate) async fn import_ssh_config_for(
        &self,
        user: LocalUser,
        path: Option<&str>,
        dry_run: bool,
        hosts: Option<&[String]>,
        folder_id: Option<FolderId>,
    ) -> Result<Outcome, EngineError> {
        let db = self.db()?;
        let path = match path {
            Some(path) => PathBuf::from(path),
            None => user
                .home
                .as_ref()
                .map(|home| home.join(".ssh").join("config"))
                .ok_or_else(|| EngineError::SshConfigUnreadable {
                    path: "~/.ssh/config".to_owned(),
                    source: io::Error::new(io::ErrorKind::NotFound, "no home directory"),
                })?,
        };
        let shown = path.display().to_string();
        // Canonical paths, the key guard and the parser: blocking file I/O, includes and all.
        let mcp = read_guard::from_mcp().then(|| read_guard::current(&BackendId::Local));
        let (found, canonical) =
            tokio::task::spawn_blocking(move || read_config(&path, &user, mcp))
                .await
                .map_err(|error| EngineError::SshConfigUnreadable {
                    path: shown.clone(),
                    source: io::Error::other(error),
                })??;
        if let Some(alias) = hosts.into_iter().flatten().find(|name| {
            !found
                .hosts
                .iter()
                .any(|host| host.alias.eq_ignore_ascii_case(name.trim()))
        }) {
            return Err(EngineError::SshConfigHostUnknown {
                alias: alias.clone(),
            });
        }
        let mut candidates = ssh_config_apply::candidates(&found, hosts);
        let target = folder_id.map_or_else(
            || ImportTarget::RootFolderNamed(DEFAULT_FOLDER.to_owned()),
            ImportTarget::Folder,
        );
        let source = ssh_config_apply::source_of(&canonical);
        let held = if read_guard::from_mcp() {
            self.hold_secret_endpoints(&source, &target, &mut candidates)
                .await?
        } else {
            Vec::new()
        };
        let mut result = db
            .import_connections(&source, &target, &candidates, dry_run)
            .await?;
        for index in held {
            result.statuses[index] = ImportStatus::Refused {
                code: "endpoint_change_needs_gui".to_owned(),
            };
        }
        if let (true, Some(folder)) = (result.folder_created, result.folder) {
            self.announce(TreeItem::Folder(folder), TreeChange::Created);
        }
        for status in &result.statuses {
            match status {
                ImportStatus::Created { id } => {
                    self.announce(TreeItem::Connection(*id), TreeChange::Created);
                }
                ImportStatus::Updated { id } => {
                    self.announce(TreeItem::Connection(*id), TreeChange::Updated);
                }
                _ => {}
            }
        }
        Ok(Outcome::SshConfigImported(ssh_config_apply::report(
            shown,
            dry_run,
            &found,
            &candidates,
            result,
        )))
    }
}

impl Engine {
    /// Deselects every candidate that would update a connection holding a stored secret, or
    /// running a login script that sends one; their indices.
    pub(super) async fn hold_secret_endpoints(
        &self,
        source: &str,
        target: &ImportTarget,
        candidates: &mut [ImportCandidate],
    ) -> Result<Vec<usize>, EngineError> {
        let db = self.db()?;
        let plan = db
            .import_connections(source, target, candidates, true)
            .await?;
        let mut held = Vec::new();
        for (index, status) in plan.statuses.iter().enumerate() {
            if let ImportStatus::Update { id } = status
                && self.holds_secrets(&db.connection(*id).await?).await
            {
                candidates[index].selected = false;
                held.push(index);
            }
        }
        Ok(held)
    }
}

/// The configuration at `path` and its canonical path, when `path` is one the import reads.
/// `mcp` is the read guard of an MCP call (itself `None` when no judge is attached), `None` for
/// the window: it is taken before the blocking thread, which sees no task-local.
fn read_config(
    path: &Path,
    user: &LocalUser,
    mcp: Option<Option<ReadGuard>>,
) -> Result<(SshConfigImport, PathBuf), EngineError> {
    let shown = || path.display().to_string();
    let canonical = match fs::canonicalize(path) {
        Ok(canonical) => canonical,
        // Only a place the import may read learns that its file is missing; any other path is
        // refused alike, whether it exists or not.
        Err(source) if config_home(path, user.home.as_deref()).is_some() => {
            return Err(EngineError::SshConfigUnreadable {
                path: shown(),
                source,
            });
        }
        Err(_) => return Err(EngineError::SshConfigPathRefused { path: shown() }),
    };
    let home = user
        .home
        .as_deref()
        .map(|home| fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf()));
    let config_of = config_home(&canonical, home.as_deref())
        .ok_or_else(|| EngineError::SshConfigPathRefused { path: shown() })?;
    // `~` and `%d` stand for the home as the person spells it, the canonical one only when the
    // given path does not name it (a link into `~/.ssh`) - a WSL share's without its verbatim
    // prefix, so its keys can be taken into the vault.
    let context_home = config_home(path, user.home.as_deref()).unwrap_or(config_of);
    let context = context_for(&plain_wsl(&context_home), user.home.as_deref(), &user.name);
    // Under MCP an include stays inside `.ssh` and passes the read guard (RC-080-12).
    let scope = mcp.map(|guard| McpScope::new(&context.ssh_dir, guard));
    let found =
        ssh_config::load_scoped(&canonical, &context, scope).map_err(|error| match error {
            LoadError::Io(source) => EngineError::SshConfigUnreadable {
                path: shown(),
                source,
            },
            LoadError::KeyMaterial { file } => EngineError::SshConfigKeyMaterial {
                path: file.display().to_string(),
            },
        })?;
    Ok((found, canonical))
}

#[cfg(test)]
#[path = "ssh_import_tests.rs"]
mod tests;
