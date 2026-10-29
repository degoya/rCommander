//! Connections read from a source that already knows them (RC-050-06): the plan a preview shows
//! and the one transaction that carries it out.
//!
//! A source names each of its connections by a key, stored with the connection
//! (`import_source`, `import_key`, migration 0007). A candidate whose key the source imported
//! before is that connection again: unchanged, or updated with the source's host, port, user
//! and key path. What the person may have changed since - name, folder, auth method, view,
//! notes - stays. Any other candidate is new and goes into the target folder.
//!
//! A dry run reads only. A real run plans and writes inside one `BEGIN IMMEDIATE` transaction,
//! so a second process importing at the same moment cannot add a duplicate.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use sqlx::{Connection as _, SqliteConnection};

use crate::{
    Db,
    connections::{
        Connection, ConnectionDraft, ConnectionError, ConnectionId, FolderId, MAX_DEPTH,
        TreeResult, connection_from_row,
        folders::require_folder,
        select_connections,
        validate::{self, Fields},
        write_error,
    },
};

/// One connection a source offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportCandidate {
    /// Its key in the source (the ssh_config import: the host alias in ASCII lower case).
    pub key: String,
    /// The connection as the source describes it; the name counts only when it is created.
    pub draft: ConnectionDraft,
    /// Whether to write it. An unselected candidate is planned and reported, never written.
    pub selected: bool,
}

/// Where new connections go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportTarget {
    /// This folder, which must exist.
    Folder(FolderId),
    /// The root's folder of this name (ignoring ASCII case), created with the first new
    /// connection when there is none.
    RootFolderNamed(String),
    /// The folders of these names from the root down (each ignoring ASCII case), those missing
    /// created with the first new connection (RC-050-09).
    RootPath(Vec<String>),
}

/// What happens, or happened, to one candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ImportStatus {
    /// Not imported yet: created when selected, unless it is a dry run.
    New,
    /// Created by this import.
    Created { id: ConnectionId },
    /// Imported before, and the source has other values now.
    Update { id: ConnectionId },
    /// Updated by this import.
    Updated { id: ConnectionId },
    /// Imported before, with the values the source has.
    Unchanged { id: ConnectionId },
    /// Not importable. `code` is a [`super::ConnectionError`] code: `name_taken` when the
    /// target folder (or an earlier candidate) has the name, or the field the validation refused.
    Refused { code: String },
}

/// What an import planned or did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportResult {
    /// The target folder; `None` while a named one does not exist yet.
    pub folder: Option<FolderId>,
    /// Whether this import created the named target folder.
    pub folder_created: bool,
    /// One per candidate, in the candidates' order.
    pub statuses: Vec<ImportStatus>,
}

impl Db {
    /// Imports `candidates` from `source`: with `dry_run` only the plan, otherwise the selected
    /// candidates written in one transaction.
    ///
    /// # Errors
    ///
    /// The target folder missing, its name blank, or the database fails. A single candidate
    /// that cannot be imported is [`ImportStatus::Refused`], not an error.
    pub async fn import_connections(
        &self,
        source: &str,
        target: &ImportTarget,
        candidates: &[ImportCandidate],
        dry_run: bool,
    ) -> TreeResult<ImportResult> {
        if dry_run {
            let mut reader = self.readers().acquire().await?;
            let planned = plan(&mut reader, source, target, candidates).await?;
            return Ok(ImportResult {
                folder: planned.folder,
                folder_created: false,
                statuses: planned.statuses,
            });
        }
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let Plan {
            mut folder,
            mut statuses,
            fields,
        } = plan(&mut tx, source, target, candidates).await?;
        let mut folder_created = false;
        for ((candidate, status), fields) in candidates.iter().zip(&mut statuses).zip(&fields) {
            let (true, Some(fields)) = (candidate.selected, fields) else {
                continue;
            };
            let done = match status {
                ImportStatus::New => {
                    let target_folder = match folder {
                        Some(existing) => existing,
                        None => {
                            let created = create_target_folder(&mut tx, target).await?;
                            folder_created = true;
                            folder = Some(created);
                            created
                        }
                    };
                    let id = insert(&mut tx, target_folder, fields, source, &candidate.key).await?;
                    ImportStatus::Created { id }
                }
                ImportStatus::Update { id } => {
                    let id = *id;
                    update(&mut tx, id, fields).await?;
                    ImportStatus::Updated { id }
                }
                _ => continue,
            };
            *status = done;
        }
        tx.commit().await?;
        Ok(ImportResult {
            folder,
            folder_created,
            statuses,
        })
    }
}

/// The plan, and each candidate's validated columns (`None` when refused).
struct Plan {
    folder: Option<FolderId>,
    statuses: Vec<ImportStatus>,
    fields: Vec<Option<Fields>>,
}

async fn plan(
    connection: &mut SqliteConnection,
    source: &str,
    target: &ImportTarget,
    candidates: &[ImportCandidate],
) -> TreeResult<Plan> {
    let folder = resolve_target(connection, target).await?;
    let imported = imported(connection, source).await?;
    let mut taken = match folder {
        Some(folder) => names_in(connection, folder).await?,
        None => HashSet::new(),
    };
    let mut seen = HashSet::new();
    let mut statuses = Vec::with_capacity(candidates.len());
    let mut all_fields = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let fields = Fields::from_draft(&candidate.draft);
        let status = match (&fields, imported.get(&candidate.key)) {
            (Err(error), _) => ImportStatus::Refused {
                code: error.code().to_owned(),
            },
            _ if !seen.insert(candidate.key.as_str()) => name_taken(),
            (Ok(fields), Some(existing)) if same_source_values(existing, fields) => {
                ImportStatus::Unchanged { id: existing.id }
            }
            (Ok(_), Some(existing)) => ImportStatus::Update { id: existing.id },
            (Ok(fields), None) => {
                let name = fields.name.to_ascii_lowercase();
                if taken.contains(&name) {
                    name_taken()
                } else {
                    if candidate.selected {
                        taken.insert(name);
                    }
                    ImportStatus::New
                }
            }
        };
        let refused = matches!(status, ImportStatus::Refused { .. });
        statuses.push(status);
        all_fields.push(fields.ok().filter(|_| !refused));
    }
    Ok(Plan {
        folder,
        statuses,
        fields: all_fields,
    })
}

fn name_taken() -> ImportStatus {
    ImportStatus::Refused {
        code: "name_taken".to_owned(),
    }
}

/// Whether `existing` already has the values an import writes.
fn same_source_values(existing: &Connection, fields: &Fields) -> bool {
    existing.host == fields.host
        && existing.port == fields.port
        && existing.username == fields.username
        && existing.key_path == fields.key_path
}

/// The target folder, `None` for a named one that does not exist yet.
async fn resolve_target(
    connection: &mut SqliteConnection,
    target: &ImportTarget,
) -> TreeResult<Option<FolderId>> {
    match target {
        ImportTarget::Folder(id) => {
            require_folder(connection, *id).await?;
            Ok(Some(*id))
        }
        ImportTarget::RootFolderNamed(name) => {
            find_folder(connection, None, &validate::name(name)?).await
        }
        ImportTarget::RootPath(names) => {
            let mut folder = None;
            for name in names {
                let name = validate::name(name)?;
                match find_folder(connection, folder, &name).await? {
                    Some(found) => folder = Some(found),
                    None => return Ok(None),
                }
            }
            Ok(folder)
        }
    }
}

/// The folder `name` (ignoring ASCII case) in `parent`, the root for `None`.
async fn find_folder(
    connection: &mut SqliteConnection,
    parent: Option<FolderId>,
    name: &str,
) -> TreeResult<Option<FolderId>> {
    let id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM connection_folders WHERE parent_id IS ? AND name = ? COLLATE NOCASE",
    )
    .bind(parent.map(|parent| parent.0))
    .bind(name)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(id.map(FolderId))
}

/// The connections `source` imported before, by key.
async fn imported(
    connection: &mut SqliteConnection,
    source: &str,
) -> TreeResult<HashMap<String, Connection>> {
    let keys: HashMap<i64, String> = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, import_key FROM connections \
         WHERE import_source = ? AND import_key IS NOT NULL",
    )
    .bind(source)
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .collect();
    let rows = sqlx::query(select_connections!(
        "WHERE import_source = ? AND import_key IS NOT NULL"
    ))
    .bind(source)
    .fetch_all(&mut *connection)
    .await?;
    let mut by_key = HashMap::with_capacity(rows.len());
    for row in &rows {
        let found = connection_from_row(row)?;
        if let Some(key) = keys.get(&found.id.0) {
            by_key.insert(key.clone(), found);
        }
    }
    Ok(by_key)
}

/// The names of `folder`'s connections, in ASCII lower case like the unique index compares.
async fn names_in(
    connection: &mut SqliteConnection,
    folder: FolderId,
) -> TreeResult<HashSet<String>> {
    let names: Vec<String> = sqlx::query_scalar("SELECT name FROM connections WHERE folder_id = ?")
        .bind(folder.0)
        .fetch_all(&mut *connection)
        .await?;
    Ok(names
        .into_iter()
        .map(|name| name.to_ascii_lowercase())
        .collect())
}

/// The target folder, created at the root when it is a named one; a path's missing folders
/// created from the first one missing down.
async fn create_target_folder(
    connection: &mut SqliteConnection,
    target: &ImportTarget,
) -> TreeResult<FolderId> {
    let names = match target {
        ImportTarget::Folder(id) => return Ok(*id),
        ImportTarget::RootFolderNamed(name) => std::slice::from_ref(name),
        ImportTarget::RootPath(names) => names.as_slice(),
    };
    if names.len() > MAX_DEPTH {
        return Err(ConnectionError::FolderTooDeep { max: MAX_DEPTH });
    }
    let mut folder = None;
    for name in names {
        let name = validate::name(name)?;
        folder = Some(match find_folder(connection, folder, &name).await? {
            Some(found) => found,
            None => insert_folder(connection, folder, &name).await?,
        });
    }
    folder.ok_or(ConnectionError::NameEmpty)
}

/// A new folder `name` after `parent`'s last folder.
async fn insert_folder(
    connection: &mut SqliteConnection,
    parent: Option<FolderId>,
    name: &str,
) -> TreeResult<FolderId> {
    let parent = parent.map(|parent| parent.0);
    let done = sqlx::query(
        "INSERT INTO connection_folders (parent_id, name, sort_order) VALUES (?, ?, \
         (SELECT ifnull(max(sort_order) + 1, 0) FROM connection_folders WHERE parent_id IS ?))",
    )
    .bind(parent)
    .bind(name)
    .bind(parent)
    .execute(&mut *connection)
    .await
    .map_err(|error| write_error(error, name))?;
    Ok(FolderId(done.last_insert_rowid()))
}

/// A new connection after `folder`'s last one, marked with its source and key.
async fn insert(
    connection: &mut SqliteConnection,
    folder: FolderId,
    fields: &Fields,
    source: &str,
    key: &str,
) -> TreeResult<ConnectionId> {
    let done = sqlx::query(
        "INSERT INTO connections (folder_id, name, kind, host, port, username, auth_method, \
         key_path, default_view, start_directory, notes, sort_order, import_source, import_key) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, \
         (SELECT ifnull(max(sort_order) + 1, 0) FROM connections WHERE folder_id = ?), ?, ?)",
    )
    .bind(folder.0)
    .bind(&fields.name)
    .bind(fields.kind.as_str())
    .bind(&fields.host)
    .bind(i64::from(fields.port))
    .bind(&fields.username)
    .bind(fields.auth_method.as_str())
    .bind(&fields.key_path)
    .bind(fields.default_view.as_str())
    .bind(&fields.start_directory)
    .bind(&fields.notes)
    .bind(folder.0)
    .bind(source)
    .bind(key)
    .execute(&mut *connection)
    .await
    .map_err(|error| write_error(error, &fields.name))?;
    Ok(ConnectionId(done.last_insert_rowid()))
}

/// Writes the values a source owns over an imported connection.
async fn update(
    connection: &mut SqliteConnection,
    id: ConnectionId,
    fields: &Fields,
) -> TreeResult<()> {
    sqlx::query(
        "UPDATE connections SET host = ?, port = ?, username = ?, key_path = ?, \
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
    )
    .bind(&fields.host)
    .bind(i64::from(fields.port))
    .bind(&fields.username)
    .bind(&fields.key_path)
    .bind(id.0)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;
