//! Server inventories (RC-100-39, migration 0026): the sources a person set up - an inventory
//! plugin, the address it reaches, the folder its connections go into - and the values of its
//! secret slots, sealed by the caller as a login script's secrets are. Keeping the folder in step
//! with what the plugin lists is `sync.rs`.

use serde::{Deserialize, Serialize};
use sqlx::{Connection as _, SqliteConnection};

use crate::{
    Db, DbError,
    connections::{ConnectionId, FolderId},
};

mod folders;
mod sync;

pub use sync::{InventoryEntry, InventorySync, SyncStatus};

/// The table of a source's secrets; the first part of their associated data.
pub const INVENTORY_SECRETS_TABLE: &str = "inventory_source_secrets";
/// The column that holds a source secret's ciphertext.
pub const INVENTORY_SECRET_COLUMN: &str = "value_enc";
/// The longest source name, address and slot name stored.
pub const MAX_SOURCE_NAME_CHARS: usize = 255;
pub const MAX_SOURCE_ADDRESS_CHARS: usize = 2048;
const MAX_SLOT_CHARS: usize = 32;

/// A source's id. Never reused once deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InventorySourceId(pub i64);

impl std::fmt::Display for InventorySourceId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl InventorySourceId {
    /// What its connections carry as `import_source`.
    #[must_use]
    pub fn import_source(self) -> String {
        format!("inventory:{}", self.0)
    }
}

/// One inventory source, without a secret: which slots hold a value is all it says of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventorySource {
    pub id: InventorySourceId,
    pub name: String,
    /// The inventory plugin's id (a UUIDv7).
    pub plugin_id: String,
    /// The address the person entered and confirmed: the only one the plugin's `http` reaches.
    pub address: String,
    /// The folder its connections go into; `None` until the first sync creates one.
    pub folder_id: Option<FolderId>,
    /// When the last sync ran (RFC 3339, UTC); `None` before the first one, which is previewed.
    pub last_sync_at: Option<String>,
    /// How it ended: `ok` or a failure code.
    pub last_outcome: Option<String>,
    /// The slots a value is stored for, by name.
    pub slots: Vec<String>,
}

/// A new source, or the new values of one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventorySourceDraft {
    pub name: String,
    pub plugin_id: String,
    pub address: String,
    pub folder_id: Option<FolderId>,
}

/// A connection a source keeps, as the window marks it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryLink {
    pub connection: ConnectionId,
    pub source: InventorySourceId,
    /// The inventory's id of the host.
    pub external_id: String,
    /// When a sync first missed it; `None` while the inventory lists it.
    pub missing_since: Option<String>,
}

/// A refused or failed change to the inventory sources, each with a stable code.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InventoryError {
    #[error("there is no inventory source with id {id}")]
    NotFound { id: InventorySourceId },
    #[error("a name must not be empty, longer than 255 characters or hold a control character")]
    NameInvalid,
    #[error("the name {name:?} is already taken")]
    NameTaken { name: String },
    #[error("the plugin id is not a plugin's")]
    PluginInvalid,
    #[error("the address is empty, too long or holds a control character")]
    AddressInvalid,
    #[error("a slot is 1 to 32 of a-z, 0-9 and _")]
    SlotInvalid,
    #[error(transparent)]
    Db(#[from] DbError),
}

impl InventoryError {
    /// Every code, in declaration order.
    pub const ALL_CODES: &[&str] = &[
        "inventory_source_not_found",
        "name_invalid",
        "name_taken",
        "inventory_plugin_invalid",
        "inventory_address_invalid",
        "inventory_slot_invalid",
        "database_error",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound { .. } => "inventory_source_not_found",
            Self::NameInvalid => "name_invalid",
            Self::NameTaken { .. } => "name_taken",
            Self::PluginInvalid => "inventory_plugin_invalid",
            Self::AddressInvalid => "inventory_address_invalid",
            Self::SlotInvalid => "inventory_slot_invalid",
            Self::Db(_) => "database_error",
        }
    }
}

impl From<sqlx::Error> for InventoryError {
    fn from(error: sqlx::Error) -> Self {
        Self::Db(DbError::Sqlx(error))
    }
}

/// The result of every inventory call.
pub type InventoryResult<T> = Result<T, InventoryError>;

type SourceRow = (
    i64,
    String,
    String,
    String,
    Option<i64>,
    Option<String>,
    Option<String>,
);

/// `SELECT <every source column> FROM inventory_sources <tail>`, as one static string.
macro_rules! select_sources {
    ($tail:literal) => {
        concat!(
            "SELECT id, name, plugin_id, address, folder_id, last_sync_at, last_outcome \
             FROM inventory_sources ",
            $tail
        )
    };
}

impl Db {
    /// Every inventory source, by name.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn inventory_sources(&self) -> InventoryResult<Vec<InventorySource>> {
        let rows: Vec<SourceRow> = sqlx::query_as(select_sources!("ORDER BY name COLLATE NOCASE"))
            .fetch_all(self.readers())
            .await?;
        let slots: Vec<(i64, String)> =
            sqlx::query_as("SELECT source_id, slot FROM inventory_source_secrets ORDER BY slot")
                .fetch_all(self.readers())
                .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let id = row.0;
                let own = slots
                    .iter()
                    .filter(|(source, _)| *source == id)
                    .map(|(_, slot)| slot.clone())
                    .collect();
                source_from_row(row, own)
            })
            .collect())
    }

    /// Source `id`.
    ///
    /// # Errors
    ///
    /// [`InventoryError::NotFound`], or the read fails.
    pub async fn inventory_source(
        &self,
        id: InventorySourceId,
    ) -> InventoryResult<InventorySource> {
        let mut reader = self.readers().acquire().await?;
        read_source(&mut reader, id).await
    }

    /// Adds a source; the person confirmed its address by entering it.
    ///
    /// # Errors
    ///
    /// A field refused ([`InventoryError::NameInvalid`], `NameTaken`, `PluginInvalid`,
    /// `AddressInvalid`), or the write fails.
    pub async fn create_inventory_source(
        &self,
        draft: &InventorySourceDraft,
    ) -> InventoryResult<InventorySourceId> {
        let draft = checked(draft)?;
        let mut writer = self.writer().await;
        let done = sqlx::query(
            "INSERT INTO inventory_sources (name, plugin_id, address, folder_id) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(&draft.name)
        .bind(&draft.plugin_id)
        .bind(&draft.address)
        .bind(draft.folder_id.map(|folder| folder.0))
        .execute(&mut *writer)
        .await
        .map_err(|error| taken(error, &draft.name))?;
        Ok(InventorySourceId(done.last_insert_rowid()))
    }

    /// Changes a source. A changed address or plugin drops the stored secrets: they were given
    /// for the old one (as a changed endpoint revokes an approval).
    ///
    /// # Errors
    ///
    /// As [`Db::create_inventory_source`], and [`InventoryError::NotFound`].
    pub async fn update_inventory_source(
        &self,
        id: InventorySourceId,
        draft: &InventorySourceDraft,
    ) -> InventoryResult<()> {
        let draft = checked(draft)?;
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let current = read_source(&mut tx, id).await?;
        sqlx::query(
            "UPDATE inventory_sources SET name = ?, plugin_id = ?, address = ?, folder_id = ?, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(&draft.name)
        .bind(&draft.plugin_id)
        .bind(&draft.address)
        .bind(draft.folder_id.map(|folder| folder.0))
        .bind(id.0)
        .execute(&mut *tx)
        .await
        .map_err(|error| taken(error, &draft.name))?;
        if current.address != draft.address || current.plugin_id != draft.plugin_id {
            sqlx::query("DELETE FROM inventory_source_secrets WHERE source_id = ?")
                .bind(id.0)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Removes a source with its secrets; its connections stay as ordinary ones.
    ///
    /// # Errors
    ///
    /// [`InventoryError::NotFound`], or the write fails.
    pub async fn delete_inventory_source(&self, id: InventorySourceId) -> InventoryResult<()> {
        let mut writer = self.writer().await;
        let done = sqlx::query("DELETE FROM inventory_sources WHERE id = ?")
            .bind(id.0)
            .execute(&mut *writer)
            .await?;
        if done.rows_affected() == 0 {
            return Err(InventoryError::NotFound { id });
        }
        Ok(())
    }

    /// The ciphertext of `slot` of `source`; `None` when none is stored. With its row id, the
    /// last part of its associated data.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn inventory_secret(
        &self,
        source: InventorySourceId,
        slot: &str,
    ) -> InventoryResult<Option<(i64, Vec<u8>)>> {
        Ok(sqlx::query_as(
            "SELECT id, value_enc FROM inventory_source_secrets WHERE source_id = ? AND slot = ?",
        )
        .bind(source.0)
        .bind(slot)
        .fetch_optional(self.readers())
        .await?)
    }

    /// Stores the value of `slot` with `source`, replacing one stored before. `seal` gets the
    /// row id for the associated data and answers the ciphertext; when it fails nothing is
    /// written and its error comes back inside `Ok`.
    ///
    /// # Errors
    ///
    /// The source missing, the slot not a slot name, [`DbError::VaultChanged`] when the vault
    /// key this process holds is no longer the vault's, or the write fails.
    pub async fn store_inventory_secret<E>(
        &self,
        source: InventorySourceId,
        slot: &str,
        seal: impl FnOnce(&str) -> Result<Vec<u8>, E>,
    ) -> InventoryResult<Result<(), E>> {
        if !valid_slot(slot) {
            return Err(InventoryError::SlotInvalid);
        }
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        read_source(&mut tx, source).await?;
        self.check_vault_key(&mut tx).await?;
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM inventory_source_secrets WHERE source_id = ? AND slot = ?",
        )
        .bind(source.0)
        .bind(slot)
        .fetch_optional(&mut *tx)
        .await?;
        let id = match existing {
            Some(id) => id,
            None => {
                // An empty value until it is sealed below, in the same transaction.
                sqlx::query(
                    "INSERT INTO inventory_source_secrets (source_id, slot, value_enc) \
                     VALUES (?, ?, zeroblob(0))",
                )
                .bind(source.0)
                .bind(slot)
                .execute(&mut *tx)
                .await?
                .last_insert_rowid()
            }
        };
        let cipher = match seal(&id.to_string()) {
            Ok(cipher) => cipher,
            Err(error) => return Ok(Err(error)),
        };
        sqlx::query(
            "UPDATE inventory_source_secrets SET value_enc = ?, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(cipher)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Ok(()))
    }

    /// The connections the sources keep, for the window's marks.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn inventory_links(&self) -> InventoryResult<Vec<InventoryLink>> {
        let rows: Vec<(i64, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, import_source, import_key, inventory_missing_since FROM connections \
             WHERE import_source LIKE 'inventory:%' AND import_key IS NOT NULL ORDER BY id",
        )
        .fetch_all(self.readers())
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, source, key, missing)| {
                let source = source.strip_prefix("inventory:")?.parse().ok()?;
                Some(InventoryLink {
                    connection: ConnectionId(id),
                    source: InventorySourceId(source),
                    external_id: key,
                    missing_since: missing,
                })
            })
            .collect())
    }
}

fn source_from_row(row: SourceRow, slots: Vec<String>) -> InventorySource {
    let (id, name, plugin_id, address, folder_id, last_sync_at, last_outcome) = row;
    InventorySource {
        id: InventorySourceId(id),
        name,
        plugin_id,
        address,
        folder_id: folder_id.map(FolderId),
        last_sync_at,
        last_outcome,
        slots,
    }
}

/// Source `id`, read inside `connection`.
pub(crate) async fn read_source(
    connection: &mut SqliteConnection,
    id: InventorySourceId,
) -> InventoryResult<InventorySource> {
    let row: Option<SourceRow> = sqlx::query_as(select_sources!("WHERE id = ?"))
        .bind(id.0)
        .fetch_optional(&mut *connection)
        .await?;
    let row = row.ok_or(InventoryError::NotFound { id })?;
    let slots: Vec<String> = sqlx::query_scalar(
        "SELECT slot FROM inventory_source_secrets WHERE source_id = ? ORDER BY slot",
    )
    .bind(id.0)
    .fetch_all(&mut *connection)
    .await?;
    Ok(source_from_row(row, slots))
}

/// The draft's fields, trimmed and checked: a name, a plugin id of 36 characters, an address
/// without a control character. Whether the address is one the plugin may reach is the
/// plugin host's to judge (`HttpGrant::entered`).
fn checked(draft: &InventorySourceDraft) -> InventoryResult<InventorySourceDraft> {
    let name = draft.name.trim();
    if name.is_empty()
        || name.chars().count() > MAX_SOURCE_NAME_CHARS
        || name.chars().any(char::is_control)
    {
        return Err(InventoryError::NameInvalid);
    }
    let plugin_id = draft.plugin_id.trim();
    if plugin_id.len() != 36
        || !plugin_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    {
        return Err(InventoryError::PluginInvalid);
    }
    let address = draft.address.trim();
    if address.is_empty()
        || address.chars().count() > MAX_SOURCE_ADDRESS_CHARS
        || address.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(InventoryError::AddressInvalid);
    }
    Ok(InventorySourceDraft {
        name: name.to_owned(),
        plugin_id: plugin_id.to_owned(),
        address: address.to_owned(),
        folder_id: draft.folder_id,
    })
}

/// A slot name: 1 to 32 of `a-z`, `0-9`, `_`, as the plugin host reads `secrets`.
fn valid_slot(slot: &str) -> bool {
    !slot.is_empty()
        && slot.len() <= MAX_SLOT_CHARS
        && slot
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// A unique-index violation on the name is [`InventoryError::NameTaken`].
fn taken(error: sqlx::Error, name: &str) -> InventoryError {
    match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => {
            InventoryError::NameTaken {
                name: name.to_owned(),
            }
        }
        _ => error.into(),
    }
}

#[cfg(test)]
#[path = "inventory_tests.rs"]
mod tests;
