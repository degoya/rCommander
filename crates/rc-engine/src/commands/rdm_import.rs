//! `import.remote_desktop_manager` (RC-050-11): the SSH connections of an export of
//! Devolutions Remote Desktop Manager into the connection tree, or with `dry_run` only the
//! preview of that.
//!
//! New connections go into the folder [`DEFAULT_FOLDER`] at the root, RDM's folders inside it.
//! An entry is known again by RDM's ID (`import_source` [`SOURCE`], the ID as key), so a second
//! import - of the same file or of a later export - updates host, port, user and key path
//! instead of adding it twice. The window reads any file the person picks; over MCP only one
//! ending in `.rdm` that the read guard lets through under [`RDM_EXPORT_PASSWORDS`] alone,
//! refused alike whether it exists or not,
//! and an update that would move a stored secret's endpoint is the window's
//! (`endpoint_change_needs_gui`). No password is ever read (`import::rdm`): stage 2 of the job,
//! RDM's `SafePassword`, is No-Go.

use std::{
    io,
    path::{Path, PathBuf},
};

use rc_db::connections::{ImportCandidate, ImportStatus, ImportTarget};
use rc_vfs::BackendId;

use crate::{
    AuthMethod, ConnectionDraft, ConnectionKind, Engine, EngineError, Outcome, TreeChange,
    TreeItem,
    import::{
        rdm::{self, RdmEntry, RdmError, RdmExport},
        rdm_report::{DEFAULT_FOLDER, RdmImportReport, RdmRow, SOURCE},
        total_commander_apply::chosen,
    },
    read_guard::{self, RDM_EXPORT_PASSWORDS, ReadGuard},
    registry::RdmImportRequest,
};

/// The entries of one folder, the way the repository imports them.
struct Group {
    folders: Vec<String>,
    /// Indices into the entries.
    members: Vec<usize>,
    candidates: Vec<ImportCandidate>,
}

/// The key of the entry at `at`: RDM's ID, or its place for one without (never imported).
fn key_of(entry: &RdmEntry, at: usize) -> String {
    entry.id.clone().unwrap_or_else(|| format!("entry{at}"))
}

/// The new connection an entry of `kind` describes: key authentication with a key path, a
/// password (asked for, never imported) without.
fn draft(entry: &RdmEntry, kind: ConnectionKind) -> ConnectionDraft {
    ConnectionDraft {
        kind,
        port: entry.port,
        username: entry.user.clone(),
        auth_method: if entry.key_path.is_some() {
            AuthMethod::Key
        } else {
            AuthMethod::Password
        },
        key_path: entry.key_path.clone(),
        ..ConnectionDraft::new(entry.name.clone(), entry.host.clone())
    }
}

/// The importable entries grouped by their folders (ignoring ASCII case), in file order;
/// selected when `only` names them or is `None`.
fn groups(entries: &[RdmEntry], only: Option<&[String]>) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for (at, entry) in entries.iter().enumerate() {
        let Ok(kind) = entry.kind else {
            continue;
        };
        let key = key_of(entry, at);
        let candidate = ImportCandidate {
            selected: chosen(&key, only, true),
            key,
            draft: draft(entry, kind),
        };
        let same = |group: &Group| {
            group.folders.len() == entry.folders.len()
                && group
                    .folders
                    .iter()
                    .zip(&entry.folders)
                    .all(|(one, other)| one.eq_ignore_ascii_case(other))
        };
        let at_group = match groups.iter().position(same) {
            Some(found) => found,
            None => {
                groups.push(Group {
                    folders: entry.folders.clone(),
                    members: Vec::new(),
                    candidates: Vec::new(),
                });
                groups.len() - 1
            }
        };
        groups[at_group].members.push(at);
        groups[at_group].candidates.push(candidate);
    }
    groups
}

fn target(folders: &[String]) -> ImportTarget {
    let mut path = vec![DEFAULT_FOLDER.to_owned()];
    path.extend(folders.iter().cloned());
    ImportTarget::RootPath(path)
}

/// The export at `path`. `mcp` is the read guard of an MCP call (itself `None` when no judge is
/// attached), `None` for the window: it is taken before the blocking thread, which sees no
/// task-local.
fn read(path: &Path, mcp: Option<Option<ReadGuard>>) -> Result<RdmExport, EngineError> {
    let shown = || path.display().to_string();
    if let Some(guard) = mcp {
        let named = path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("rdm"));
        // The guard refuses every `.rdm` to the reading tools; this reader alone takes it under
        // that one reason, as it never answers a password - a link of that name to a key is
        // still refused.
        let refused = guard.is_some_and(|guard| {
            guard
                .refuses(path)
                .is_some_and(|reason| reason != RDM_EXPORT_PASSWORDS)
        });
        if !named || refused {
            return Err(EngineError::RdmPathRefused { path: shown() });
        }
    }
    rdm::read_file(path).map_err(|error| match error {
        RdmError::TooLarge => EngineError::RdmTooLarge {
            path: shown(),
            max: rdm::MAX_FILE_BYTES,
        },
        RdmError::NotAFile => EngineError::RdmUnreadable {
            path: shown(),
            source: io::Error::other("not a regular file"),
        },
        RdmError::Io(source) => EngineError::RdmUnreadable {
            path: shown(),
            source,
        },
        RdmError::Encrypted => EngineError::RdmEncrypted { path: shown() },
        RdmError::Invalid { line } => EngineError::RdmInvalid {
            path: shown(),
            line,
        },
        RdmError::NotAnExport => EngineError::RdmNotAnExport { path: shown() },
    })
}

/// A report row of `entry`, before the repository's plan.
fn row(entry: &RdmEntry, at: usize) -> RdmRow {
    RdmRow {
        key: key_of(entry, at),
        name: entry.name.clone(),
        folders: entry.folders.clone(),
        rdm_type: entry.rdm_type.clone(),
        kind: entry.kind.ok(),
        host: entry.host.clone(),
        port: entry.port,
        user: entry.user.clone(),
        key_path: entry.key_path.clone(),
        key_path_refused: entry.key_path_refused,
        password_state: entry.password,
        selected: false,
        status: match entry.kind {
            Ok(_) => ImportStatus::New,
            Err(skip) => ImportStatus::Refused {
                code: skip.code().to_owned(),
            },
        },
    }
}

impl Engine {
    /// `import.remote_desktop_manager`.
    pub(crate) async fn import_rdm(
        &self,
        request: &RdmImportRequest,
    ) -> Result<Outcome, EngineError> {
        let db = self.db()?;
        let path = PathBuf::from(request.path.trim());
        let shown = path.display().to_string();
        let mcp = read_guard::from_mcp().then(|| read_guard::current(&BackendId::Local));
        let found = tokio::task::spawn_blocking(move || read(&path, mcp))
            .await
            .map_err(|error| EngineError::RdmUnreadable {
                path: shown.clone(),
                source: io::Error::other(error),
            })??;
        let only = request.only.as_deref();
        let mut rows: Vec<RdmRow> = found
            .entries
            .iter()
            .enumerate()
            .map(|(at, entry)| row(entry, at))
            .collect();
        for mut group in groups(&found.entries, only) {
            let target = target(&group.folders);
            let held = if read_guard::from_mcp() {
                self.hold_secret_endpoints(SOURCE, &target, &mut group.candidates)
                    .await?
            } else {
                Vec::new()
            };
            let mut result = db
                .import_connections(SOURCE, &target, &group.candidates, request.dry_run)
                .await?;
            for index in held {
                result.statuses[index] = ImportStatus::Refused {
                    code: "endpoint_change_needs_gui".to_owned(),
                };
            }
            if let (true, Some(folder)) = (result.folder_created, result.folder) {
                self.announce(TreeItem::Folder(folder), TreeChange::Created);
            }
            for ((at, candidate), status) in group
                .members
                .iter()
                .zip(&group.candidates)
                .zip(result.statuses)
            {
                match &status {
                    ImportStatus::Created { id } => {
                        self.announce(TreeItem::Connection(*id), TreeChange::Created);
                    }
                    ImportStatus::Updated { id } => {
                        self.announce(TreeItem::Connection(*id), TreeChange::Updated);
                    }
                    _ => {}
                }
                rows[*at].selected = candidate.selected;
                rows[*at].status = status;
            }
        }
        let warnings = if found.truncated {
            vec!["too_many_entries".to_owned()]
        } else {
            Vec::new()
        };
        Ok(Outcome::RdmImported(RdmImportReport {
            path: shown,
            dry_run: request.dry_run,
            entries: rows,
            warnings,
        }))
    }
}

#[cfg(test)]
#[path = "rdm_import_tests.rs"]
mod tests;
