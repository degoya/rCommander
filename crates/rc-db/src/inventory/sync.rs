//! Keeping a source's folder in step with what its inventory lists (RC-100-39): the plan a
//! preview shows and the one transaction that carries it out, as a connection import is
//! (`connections/import.rs`).
//!
//! A host is matched by the inventory's own id (`import_key`), never by its name. The fields the
//! inventory owns - name, address, port and folder - are written over; everything else of a
//! connection is the person's and stays: credentials and auth method, login script, the
//! drive-bar mark, view, notes, theme. The user, protocol and note a host comes with are taken
//! when the connection is created, and left alone afterwards. A host the inventory no longer
//! lists is marked (`inventory_missing_since`), never deleted, and unmarked when it comes back;
//! one the plugin listed but the host could not read (`unread`) is neither. A second sync of the
//! same list writes nothing.
//!
//! Two hosts swapping their names in one folder meet the name index halfway: the first is
//! refused (`name_taken`) and the next sync takes it.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use sqlx::{Connection as _, SqliteConnection};

use super::{
    InventoryResult, InventorySourceId,
    folders::{subfolder, target_folder},
    read_source,
};
use crate::{
    Db,
    connections::{
        AuthMethod, ConnectionDraft, ConnectionId, ConnectionKind, DefaultView, FolderId,
        check_draft,
    },
};

/// One host as a sync takes it: the plugin's answer, checked by the plugin host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryEntry {
    /// The inventory's id of the host.
    pub external_id: String,
    pub name: String,
    pub address: String,
    pub port: u16,
    pub kind: ConnectionKind,
    /// What the connection opens in: the terminal for `ssh`, the files otherwise.
    pub default_view: DefaultView,
    pub user: Option<String>,
    /// The folders below the source's folder, from the top; empty for the source's folder.
    pub folder: Vec<String>,
    /// The note, with the host's tags; written when the connection is created.
    pub notes: Option<String>,
}

/// What happens, or happened, to one host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SyncStatus {
    /// Not in rCommander yet: created unless it is a preview.
    New,
    Created {
        id: ConnectionId,
    },
    /// The inventory has another name, address, port or folder, or the host was missing.
    Update {
        id: ConnectionId,
    },
    Updated {
        id: ConnectionId,
    },
    Unchanged {
        id: ConnectionId,
    },
    /// Not taken: `code` names why (`name_taken`, `duplicate`, or the field refused).
    Refused {
        code: String,
    },
}

/// What a sync planned or did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventorySync {
    /// The source's folder; `None` in a preview before the first sync made it.
    pub folder: Option<FolderId>,
    /// One per entry, in the entries' order.
    pub statuses: Vec<SyncStatus>,
    /// The connections this sync marks (or would mark) as no longer in the inventory.
    pub missing: Vec<ConnectionId>,
    /// Those marked before and still missing.
    pub still_missing: usize,
    /// Those marked before that the inventory lists again; unmarked by the update.
    pub returned: Vec<ConnectionId>,
}

impl InventorySync {
    /// How many entries end, or would end, in each state: created or new, updated or to update,
    /// unchanged, refused.
    #[must_use]
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let mut counts = (0, 0, 0, 0);
        for status in &self.statuses {
            match status {
                SyncStatus::New | SyncStatus::Created { .. } => counts.0 += 1,
                SyncStatus::Update { .. } | SyncStatus::Updated { .. } => counts.1 += 1,
                SyncStatus::Unchanged { .. } => counts.2 += 1,
                SyncStatus::Refused { .. } => counts.3 += 1,
            }
        }
        counts
    }
}

/// A connection the source kept before.
struct Linked {
    id: ConnectionId,
    name: String,
    host: String,
    port: u16,
    folder_id: Option<FolderId>,
    missing: bool,
}

impl Db {
    /// Syncs source `id` with `entries`, the hosts its inventory lists now; `unread` are the ids
    /// of hosts it listed that the plugin host left out. With `dry_run` only the plan (the
    /// preview before the first sync), otherwise one transaction that also records the sync.
    ///
    /// # Errors
    ///
    /// The source missing, or the database fails. A single host that cannot be taken is
    /// [`SyncStatus::Refused`], not an error.
    pub async fn sync_inventory(
        &self,
        id: InventorySourceId,
        entries: &[InventoryEntry],
        unread: &[String],
        dry_run: bool,
    ) -> InventoryResult<InventorySync> {
        if dry_run {
            let mut reader = self.readers().acquire().await?;
            return run(&mut reader, id, entries, unread, false).await;
        }
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let done = run(&mut tx, id, entries, unread, true).await?;
        sqlx::query(
            "UPDATE inventory_sources SET folder_id = ?, last_outcome = 'ok', \
             last_sync_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(done.folder.map(|folder| folder.0))
        .bind(id.0)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(done)
    }

    /// Records a sync that failed before it could write: `outcome` is the failure's code.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn record_inventory_outcome(
        &self,
        id: InventorySourceId,
        outcome: &str,
    ) -> InventoryResult<()> {
        let outcome: String = outcome.chars().take(64).collect();
        let mut writer = self.writer().await;
        sqlx::query(
            "UPDATE inventory_sources SET last_outcome = ?, \
             last_sync_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(outcome)
        .bind(id.0)
        .execute(&mut *writer)
        .await?;
        Ok(())
    }
}

/// Plans the sync, and with `write` carries it out on `connection` (inside a transaction).
async fn run(
    connection: &mut SqliteConnection,
    id: InventorySourceId,
    entries: &[InventoryEntry],
    unread: &[String],
    write: bool,
) -> InventoryResult<InventorySync> {
    let source = read_source(connection, id).await?;
    let mut done = InventorySync {
        folder: target_folder(connection, &source, write).await?,
        ..InventorySync::default()
    };
    let linked = linked(connection, id).await?;
    let mut folders: HashMap<Vec<String>, Option<FolderId>> = HashMap::new();
    let mut seen = HashSet::new();
    let mut planned_names = HashSet::new();
    let import_source = id.import_source();
    for entry in entries {
        if !seen.insert(entry.external_id.as_str()) {
            done.statuses.push(refused("duplicate"));
            continue;
        }
        let draft = draft(entry);
        if let Err(error) = check_draft(&draft) {
            done.statuses.push(refused(error.code()));
            continue;
        }
        let folder = match (done.folder, folders.get(&entry.folder)) {
            (_, Some(known)) => *known,
            (None, None) => None,
            (Some(top), None) => {
                let found = subfolder(connection, top, &entry.folder, write).await?;
                folders.insert(entry.folder.clone(), found);
                found
            }
        };
        let name = draft.name.trim().to_owned();
        let host = draft.host.trim().to_owned();
        let existing = linked.get(&entry.external_id);
        let key = (
            folder.map_or(0, |folder| folder.0),
            name.to_ascii_lowercase(),
        );
        let name_taken = match folder {
            Some(folder) => {
                taken(connection, folder, &name, existing.map(|linked| linked.id)).await?
                    || (!write && planned_names.contains(&key))
            }
            // A folder a preview has yet to make holds nothing but what this sync plans.
            None => planned_names.contains(&key),
        };
        let status = match existing {
            Some(linked)
                if linked.name == name
                    && linked.host == host
                    && linked.port == entry.port
                    && linked.folder_id == folder
                    && !linked.missing =>
            {
                SyncStatus::Unchanged { id: linked.id }
            }
            _ if name_taken => refused("name_taken"),
            Some(linked) => {
                if linked.missing {
                    done.returned.push(linked.id);
                }
                if write {
                    update(connection, linked.id, &name, &host, entry.port, folder).await?;
                    SyncStatus::Updated { id: linked.id }
                } else {
                    SyncStatus::Update { id: linked.id }
                }
            }
            None => match (write, folder) {
                (true, Some(folder)) => {
                    let id = insert(connection, folder, &draft, &import_source, entry).await?;
                    SyncStatus::Created { id }
                }
                _ => SyncStatus::New,
            },
        };
        if !matches!(status, SyncStatus::Refused { .. }) {
            planned_names.insert(key);
        }
        done.statuses.push(status);
    }
    let unread: HashSet<&str> = unread.iter().map(String::as_str).collect();
    for (key, linked) in &linked {
        if seen.contains(key.as_str()) || unread.contains(key.as_str()) {
            continue;
        }
        if linked.missing {
            done.still_missing += 1;
            continue;
        }
        if write {
            sqlx::query(
                "UPDATE connections SET \
                 inventory_missing_since = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
            )
            .bind(linked.id.0)
            .execute(&mut *connection)
            .await?;
        }
        done.missing.push(linked.id);
    }
    done.missing.sort();
    Ok(done)
}

fn refused(code: &str) -> SyncStatus {
    SyncStatus::Refused {
        code: code.to_owned(),
    }
}

/// The connection an entry becomes when it is created.
fn draft(entry: &InventoryEntry) -> ConnectionDraft {
    let mut draft = ConnectionDraft::new(entry.name.clone(), entry.address.clone());
    draft.kind = entry.kind;
    draft.port = Some(i64::from(entry.port));
    draft.username = entry.user.clone();
    draft.default_view = entry.default_view;
    draft.notes = entry.notes.clone();
    // FTP without a user is anonymous; with one it asks for the password, as SSH does.
    if entry.kind.is_ftp() && entry.user.is_none() {
        draft.auth_method = AuthMethod::Anonymous;
    }
    draft
}

/// Whether another connection than `own` in `folder` has `name` (ignoring ASCII case).
async fn taken(
    connection: &mut SqliteConnection,
    folder: FolderId,
    name: &str,
    own: Option<ConnectionId>,
) -> InventoryResult<bool> {
    let other: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM connections WHERE folder_id = ? AND name = ? COLLATE NOCASE \
         AND id IS NOT ? LIMIT 1",
    )
    .bind(folder.0)
    .bind(name)
    .bind(own.map(|own| own.0))
    .fetch_optional(&mut *connection)
    .await?;
    Ok(other.is_some())
}

/// The connections the source kept before, by the inventory's id.
async fn linked(
    connection: &mut SqliteConnection,
    id: InventorySourceId,
) -> InventoryResult<HashMap<String, Linked>> {
    type Row = (
        i64,
        String,
        String,
        String,
        i64,
        Option<i64>,
        Option<String>,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, import_key, name, host, port, folder_id, inventory_missing_since \
         FROM connections WHERE import_source = ? AND import_key IS NOT NULL",
    )
    .bind(id.import_source())
    .fetch_all(&mut *connection)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, key, name, host, port, folder_id, missing)| {
            let linked = Linked {
                id: ConnectionId(id),
                name,
                host,
                port: u16::try_from(port).unwrap_or(0),
                folder_id: folder_id.map(FolderId),
                missing: missing.is_some(),
            };
            (key, linked)
        })
        .collect())
}

/// Writes the fields the inventory owns, and unmarks a connection that was missing.
async fn update(
    connection: &mut SqliteConnection,
    id: ConnectionId,
    name: &str,
    host: &str,
    port: u16,
    folder: Option<FolderId>,
) -> InventoryResult<()> {
    sqlx::query(
        "UPDATE connections SET name = ?, host = ?, port = ?, folder_id = ?, \
         inventory_missing_since = NULL, \
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
    )
    .bind(name)
    .bind(host)
    .bind(i64::from(port))
    .bind(folder.map(|folder| folder.0))
    .bind(id.0)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

/// A new connection after `folder`'s last one, marked with its source and the host's id.
async fn insert(
    connection: &mut SqliteConnection,
    folder: FolderId,
    draft: &ConnectionDraft,
    source: &str,
    entry: &InventoryEntry,
) -> InventoryResult<ConnectionId> {
    let text = |value: &Option<String>| {
        value
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    };
    let done = sqlx::query(
        "INSERT INTO connections (folder_id, name, kind, host, port, username, auth_method, \
         default_view, notes, sort_order, import_source, import_key) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, \
         (SELECT ifnull(max(sort_order) + 1, 0) FROM connections WHERE folder_id = ?), ?, ?)",
    )
    .bind(folder.0)
    .bind(draft.name.trim())
    .bind(draft.kind.as_str())
    .bind(draft.host.trim())
    .bind(i64::from(entry.port))
    .bind(text(&draft.username))
    .bind(draft.auth_method.as_str())
    .bind(draft.default_view.as_str())
    .bind(text(&draft.notes))
    .bind(folder.0)
    .bind(source)
    .bind(&entry.external_id)
    .execute(&mut *connection)
    .await?;
    Ok(ConnectionId(done.last_insert_rowid()))
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
