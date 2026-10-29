//! `import.total_commander` and `import.total_commander_passwords` (RC-050-09): Total Commander's
//! directory menu, tabs and FTP connections into the hotlist, the panels and the connection tree,
//! its colours, rules by file type and list font into the settings (RC-050-14), or with `dry_run`
//! only the preview of that; and, only after the person asked, the passwords of
//! those connections straight into the vault.
//!
//! The import never reads a password's value: the report says per connection only whether
//! Total Commander keeps one ([`TcPassword`]). The second command is `GuiOnly`; it needs the vault
//! open (`vault_absent`, `vault_locked` before anything is read), decodes each obfuscated
//! password in memory that is zeroized, and stores it encrypted with the connection the import
//! made for it (found again by its `import_source` and key). One encrypted with Total
//! Commander's master password is never read: the connection comes without one.

use std::path::Path;

use rc_db::connections::{ImportCandidate, ImportStatus, ImportTarget};
use rc_vault::{ExposeSecret as _, SecretString};
use rc_vfs::BackendId;

use crate::{
    AuthMethod, ConnectionDraft, Engine, EngineError, Outcome, PanelId, SecretKind, SecretParam,
    TreeChange, TreeItem,
    import::{
        tc_password::{TcPassword, reveal},
        total_commander::{self as tc, TcConnection},
        total_commander_load::{Caller, Loaded, Wanted, load},
        total_commander_look,
        total_commander_report::{
            DEFAULT_FOLDER, SOURCE_PREFIX, TcConnectionRow, TcImportReport, TcPasswordSkip,
            TcPasswordsReport,
        },
    },
    read_guard,
    registry::{TcImportRequest, TcPasswordsRequest},
};

/// The connections of one folder, the way the repository imports them.
struct Group {
    folders: Vec<String>,
    /// Indices into the found connections.
    members: Vec<usize>,
    candidates: Vec<ImportCandidate>,
}

/// The `import_source` of the file at `path`.
fn source_of(path: &Path) -> String {
    format!("{SOURCE_PREFIX}{}", path.display())
}

/// The key of a connection, in the report and in the repository.
fn key_of(found: &TcConnection) -> String {
    format!("ftp:{}", found.section.to_ascii_lowercase())
}

/// The importable connections grouped by their folders, in file order; selected when `only`
/// names them or is `None`.
fn groups(found: &[TcConnection], only: Option<&[String]>) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for (at, connection) in found.iter().enumerate() {
        let Ok(kind) = connection.kind else {
            continue;
        };
        let anonymous = connection
            .user
            .as_deref()
            .is_none_or(|user| user.eq_ignore_ascii_case("anonymous"));
        let draft = ConnectionDraft {
            kind,
            port: connection.port.map(i64::from),
            username: connection.user.clone(),
            auth_method: if kind.is_ftp() && anonymous {
                AuthMethod::Anonymous
            } else {
                AuthMethod::Password
            },
            start_directory: connection.directory.clone(),
            ..ConnectionDraft::new(connection.name.clone(), connection.host.clone())
        };
        let key = key_of(connection);
        let candidate = ImportCandidate {
            selected: crate::import::total_commander_apply::chosen(&key, only, true),
            key,
            draft,
        };
        let folder_key: Vec<String> = connection
            .folders
            .iter()
            .map(|name| name.to_ascii_lowercase())
            .collect();
        let existing = groups.iter_mut().find(|group| {
            group
                .folders
                .iter()
                .map(|name| name.to_ascii_lowercase())
                .eq(folder_key.iter().cloned())
        });
        let group = match existing {
            Some(group) => group,
            None => {
                groups.push(Group {
                    folders: connection.folders.clone(),
                    members: Vec::new(),
                    candidates: Vec::new(),
                });
                groups.last_mut().expect("just pushed")
            }
        };
        group.members.push(at);
        group.candidates.push(candidate);
    }
    groups
}

fn target(folders: &[String]) -> ImportTarget {
    let mut path = vec![DEFAULT_FOLDER.to_owned()];
    path.extend(folders.iter().cloned());
    ImportTarget::RootPath(path)
}

/// The files of a request, read off the async threads.
async fn read(wanted: Wanted) -> Result<Loaded, EngineError> {
    let caller = Caller {
        mcp: read_guard::from_mcp(),
        guard: read_guard::current(&BackendId::Local),
    };
    tokio::task::spawn_blocking(move || load(&wanted, caller))
        .await
        .map_err(|error| EngineError::TcIniUnreadable {
            path: String::new(),
            source: std::io::Error::other(error),
        })?
}

impl Engine {
    /// `import.total_commander`.
    pub(crate) async fn import_total_commander(
        &self,
        request: &TcImportRequest,
    ) -> Result<Outcome, EngineError> {
        let parts = request.parts;
        let loaded = read(Wanted {
            wincmd_ini: request.wincmd_ini.clone(),
            ftp_ini: request.ftp_ini.clone(),
            wincmd: parts.hotlist || parts.tabs || parts.look,
            ftp: parts.connections,
        })
        .await?;
        let only = request.only.as_deref();
        let lookup = |name: &str| loaded.lookup(name);
        let (menu, tabs) = match (&loaded.wincmd, parts.hotlist || parts.tabs) {
            (Some((_, ini)), true) => {
                let menu = if parts.hotlist {
                    tc::hotlist(ini, &lookup)
                } else {
                    Vec::new()
                };
                let mut tabs = Vec::new();
                if parts.tabs {
                    for panel in PanelId::BOTH {
                        tabs.extend(tc::tabs(ini, panel, &lookup));
                    }
                }
                (menu, tabs)
            }
            _ => (Vec::new(), Vec::new()),
        };
        let looks = match (&loaded.wincmd, parts.look) {
            (Some((_, ini)), true) => {
                let mut looks = total_commander_look::colours(ini);
                looks.extend(total_commander_look::rules(ini));
                looks.extend(total_commander_look::font(ini));
                looks
            }
            _ => Vec::new(),
        };
        let mut warnings = loaded.warnings.clone();
        if parts.hotlist
            && loaded
                .wincmd
                .as_ref()
                .is_some_and(|(_, ini)| ini.get("DirMenu", "RedirectSection").is_some())
        {
            warnings.push("dir_menu_redirected".to_owned());
        }
        let ftp = loaded
            .ftp
            .as_ref()
            .filter(|_| parts.connections)
            .map(|(path, ini)| (source_of(path), tc::connections(ini)));
        let report = TcImportReport {
            dry_run: request.dry_run,
            wincmd_ini: loaded
                .wincmd
                .as_ref()
                .map(|(path, _)| path.display().to_string()),
            ftp_ini: loaded
                .ftp
                .as_ref()
                .map(|(path, _)| path.display().to_string()),
            tabs_mode: request.tabs_mode,
            warnings,
            ..TcImportReport::default()
        };
        // The files, obfuscated passwords and all, are dropped (zeroized) before anything waits.
        drop(loaded);
        let hotlist = self.import_tc_hotlist(&menu, only, request.dry_run).await?;
        let tabs = self
            .import_tc_tabs(&tabs, only, request.tabs_mode, request.dry_run)
            .await?;
        let connections = match ftp {
            Some((source, found)) => {
                self.import_tc_connections(&source, &found, only, request.dry_run)
                    .await?
            }
            None => Vec::new(),
        };
        let look = self.import_tc_look(&looks, only, request.dry_run).await?;
        Ok(Outcome::TcImported(TcImportReport {
            hotlist,
            tabs,
            connections,
            look,
            ..report
        }))
    }

    /// The connections against the tree; written unless `dry_run`.
    async fn import_tc_connections(
        &self,
        source: &str,
        found: &[TcConnection],
        only: Option<&[String]>,
        dry_run: bool,
    ) -> Result<Vec<TcConnectionRow>, EngineError> {
        let db = self.db()?;
        let mut rows: Vec<TcConnectionRow> = found
            .iter()
            .map(|connection| TcConnectionRow {
                key: key_of(connection),
                name: connection.name.clone(),
                folders: connection.folders.clone(),
                kind: connection.kind.ok(),
                host: connection.host.clone(),
                port: connection.port,
                user: connection.user.clone(),
                directory: connection.directory.clone(),
                password_state: connection.password,
                selected: false,
                status: match connection.kind {
                    Ok(_) => ImportStatus::New,
                    Err(skip) => ImportStatus::Refused {
                        code: format!("tc_{}", skip.code()),
                    },
                },
            })
            .collect();
        for mut group in groups(found, only) {
            let target = target(&group.folders);
            let held = if read_guard::from_mcp() {
                self.hold_secret_endpoints(source, &target, &mut group.candidates)
                    .await?
            } else {
                Vec::new()
            };
            let mut result = db
                .import_connections(source, &target, &group.candidates, dry_run)
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
        Ok(rows)
    }

    /// `import.total_commander_passwords`: `GuiOnly`.
    pub(crate) async fn import_total_commander_passwords(
        &self,
        request: &TcPasswordsRequest,
    ) -> Result<Outcome, EngineError> {
        // Absent or locked is answered before a file is read.
        self.vault_handle().await?.touch()?;
        let loaded = read(Wanted {
            wincmd_ini: request.wincmd_ini.clone(),
            ftp_ini: request.ftp_ini.clone(),
            wincmd: false,
            ftp: true,
        })
        .await?;
        let Some((path, ini)) = &loaded.ftp else {
            return Err(EngineError::TcIniNotFound);
        };
        let source = source_of(path);
        let found = tc::connections(ini);
        let only = request.only.as_deref();
        // Decoded only for the connections asked for, each into zeroized memory.
        let secrets: Vec<Option<SecretString>> = found
            .iter()
            .map(|connection| {
                let asked =
                    crate::import::total_commander_apply::chosen(&key_of(connection), only, true);
                if asked && connection.password == TcPassword::Present {
                    ini.get(&connection.section, "password").and_then(reveal)
                } else {
                    None
                }
            })
            .collect();
        drop(loaded);
        let mut report = TcPasswordsReport::default();
        for group in groups(&found, only) {
            let plan = self
                .db()?
                .import_connections(&source, &target(&group.folders), &group.candidates, true)
                .await?;
            for ((at, candidate), status) in group
                .members
                .iter()
                .zip(&group.candidates)
                .zip(plan.statuses)
            {
                if !candidate.selected {
                    continue;
                }
                let key = candidate.key.clone();
                let skip = |reason: &str| TcPasswordSkip {
                    key: key.clone(),
                    reason: reason.to_owned(),
                };
                let id = match status {
                    ImportStatus::Unchanged { id } | ImportStatus::Update { id } => Some(id),
                    _ => None,
                };
                let reason = match (found[*at].password, &secrets[*at], id) {
                    (TcPassword::None, ..) => Some("none"),
                    (TcPassword::MasterPassword, ..) => Some("master_password"),
                    (TcPassword::Unreadable, ..) | (_, None, _) => Some("unreadable"),
                    (_, _, None) => Some("not_imported"),
                    _ if candidate.draft.auth_method == AuthMethod::Anonymous => Some("anonymous"),
                    _ => None,
                };
                if let Some(reason) = reason {
                    report.skipped.push(skip(reason));
                    continue;
                }
                let (Some(secret), Some(id)) = (&secrets[*at], id) else {
                    continue;
                };
                let value = SecretParam::from(secret.expose_secret());
                match self.set_secret(id, SecretKind::Password, &value).await {
                    Ok(_) => report.stored.push(key),
                    Err(error @ (EngineError::VaultLocked | EngineError::Vault(_))) => {
                        return Err(error);
                    }
                    Err(error) => report.skipped.push(skip(error.code())),
                }
            }
        }
        tracing::debug!(
            stored = report.stored.len(),
            skipped = report.skipped.len(),
            "passwords taken from Total Commander"
        );
        Ok(Outcome::TcPasswordsImported(report))
    }
}

#[cfg(test)]
#[path = "tc_import_tests.rs"]
mod tests;
