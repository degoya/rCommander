//! The button bars (`button_bars`, migration 0033, RC-100-87): Total Commander's `.bar` files,
//! one row per bar with its name and its buttons as versioned JSON.
//!
//! The engine checks what goes in (the spec, the sub-bars, cycles and depth) and reads what comes
//! back as untrusted; this module keeps the text, holds the table to [`MAX_BUTTON_BARS`] rows, a
//! name to [`MAX_BUTTON_BAR_NAME`] characters and a spec to [`MAX_BUTTON_BAR_SPEC`] bytes. The
//! table is durable ([`crate::DURABLE_TABLES`]); the migration made the bar "Default".

use sqlx::{Connection as _, Row as _, SqliteConnection, sqlite::SqliteRow};

use crate::{Db, DbError};

/// The most button bars kept.
pub const MAX_BUTTON_BARS: usize = 64;

/// The longest name of a bar, in characters.
pub const MAX_BUTTON_BAR_NAME: usize = 64;

/// The largest spec of a bar, in bytes of its JSON.
pub const MAX_BUTTON_BAR_SPEC: usize = 64 * 1024;

/// One button bar as stored: its spec is the engine's JSON, unread here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredButtonBar {
    pub id: i64,
    pub name: String,
    pub spec: String,
}

/// Why a change of the button bars was refused or failed.
#[derive(Debug, thiserror::Error)]
pub enum ButtonBarStoreError {
    /// No bar has this id.
    #[error("there is no button bar {id}")]
    NotFound { id: i64 },
    /// The table holds its most bars already.
    #[error("at most {max} button bars are kept")]
    TooMany { max: usize },
    /// The name is empty, too long or holds a control character.
    #[error("a bar's name has 1 to {max} characters and no control character")]
    Name { max: usize },
    /// The spec is larger than [`MAX_BUTTON_BAR_SPEC`] bytes.
    #[error("a bar's buttons take at most {max} bytes")]
    SpecTooLarge { max: usize },
    #[error(transparent)]
    Db(#[from] DbError),
}

impl From<sqlx::Error> for ButtonBarStoreError {
    fn from(error: sqlx::Error) -> Self {
        Self::Db(DbError::Sqlx(error))
    }
}

impl ButtonBarStoreError {
    /// Whether a write failed on a unique constraint: a name another bar has.
    #[must_use]
    pub fn is_unique_violation(&self) -> bool {
        matches!(self, Self::Db(error) if error.is_unique_violation())
    }
}

type StoreResult<T> = Result<T, ButtonBarStoreError>;

/// Refuses a name that is empty, too long or holds a control character.
fn check_name(name: &str) -> StoreResult<()> {
    let length = name.chars().count();
    if length == 0 || length > MAX_BUTTON_BAR_NAME || name.chars().any(char::is_control) {
        return Err(ButtonBarStoreError::Name {
            max: MAX_BUTTON_BAR_NAME,
        });
    }
    Ok(())
}

/// Refuses a spec larger than [`MAX_BUTTON_BAR_SPEC`] bytes.
fn check_spec(spec: &str) -> StoreResult<()> {
    if spec.len() > MAX_BUTTON_BAR_SPEC {
        return Err(ButtonBarStoreError::SpecTooLarge {
            max: MAX_BUTTON_BAR_SPEC,
        });
    }
    Ok(())
}

fn bar_from_row(row: &SqliteRow) -> sqlx::Result<StoredButtonBar> {
    Ok(StoredButtonBar {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        spec: row.try_get("spec")?,
    })
}

impl Db {
    /// Every button bar, by id: the bar "Default" of the first start first.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn button_bars(&self) -> StoreResult<Vec<StoredButtonBar>> {
        let rows = sqlx::query("SELECT id, name, spec FROM button_bars ORDER BY id")
            .fetch_all(self.readers())
            .await?;
        Ok(rows
            .iter()
            .map(bar_from_row)
            .collect::<sqlx::Result<Vec<_>>>()?)
    }

    /// The button bar `id`; `None` when there is none.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn button_bar(&self, id: i64) -> StoreResult<Option<StoredButtonBar>> {
        let row = sqlx::query("SELECT id, name, spec FROM button_bars WHERE id = ?")
            .bind(id)
            .fetch_optional(self.readers())
            .await?;
        Ok(row.as_ref().map(bar_from_row).transpose()?)
    }

    /// Stores a bar as a new one (`id` `None`) or over the bar `id`, and returns it as stored. The
    /// engine checks the spec; a name another bar has fails the unique constraint.
    ///
    /// # Errors
    ///
    /// [`ButtonBarStoreError::NotFound`], [`ButtonBarStoreError::TooMany`] for a new one beyond
    /// [`MAX_BUTTON_BARS`], a name or spec that does not fit, a taken name, or the write fails;
    /// nothing is stored.
    pub async fn save_button_bar(
        &self,
        id: Option<i64>,
        name: &str,
        spec: &str,
    ) -> StoreResult<StoredButtonBar> {
        check_name(name)?;
        check_spec(spec)?;
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let id = match id {
            Some(id) => {
                fetch(&mut tx, id).await?;
                sqlx::query(
                    "UPDATE button_bars SET name = ?, spec = ?, \
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
                )
                .bind(name)
                .bind(spec)
                .bind(id)
                .execute(&mut *tx)
                .await?;
                id
            }
            None => {
                let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM button_bars")
                    .fetch_one(&mut *tx)
                    .await?;
                if usize::try_from(count).unwrap_or(usize::MAX) >= MAX_BUTTON_BARS {
                    return Err(ButtonBarStoreError::TooMany {
                        max: MAX_BUTTON_BARS,
                    });
                }
                sqlx::query("INSERT INTO button_bars (name, spec) VALUES (?, ?)")
                    .bind(name)
                    .bind(spec)
                    .execute(&mut *tx)
                    .await?
                    .last_insert_rowid()
            }
        };
        let stored = fetch(&mut tx, id).await?;
        tx.commit().await?;
        Ok(stored)
    }

    /// Gives the bar `id` the name `name`; the bar as stored.
    ///
    /// # Errors
    ///
    /// [`ButtonBarStoreError::NotFound`], a name that does not fit or another bar has, or the
    /// write fails.
    pub async fn rename_button_bar(&self, id: i64, name: &str) -> StoreResult<StoredButtonBar> {
        check_name(name)?;
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        fetch(&mut tx, id).await?;
        sqlx::query(
            "UPDATE button_bars SET name = ?, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(name)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        let stored = fetch(&mut tx, id).await?;
        tx.commit().await?;
        Ok(stored)
    }

    /// Removes the bar `id`; whether there was one. The engine refuses a bar another opens.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn delete_button_bar(&self, id: i64) -> StoreResult<bool> {
        let mut writer = self.writer().await;
        let deleted = sqlx::query("DELETE FROM button_bars WHERE id = ?")
            .bind(id)
            .execute(&mut *writer)
            .await?
            .rows_affected();
        Ok(deleted > 0)
    }
}

async fn fetch(connection: &mut SqliteConnection, id: i64) -> StoreResult<StoredButtonBar> {
    let row = sqlx::query("SELECT id, name, spec FROM button_bars WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or(ButtonBarStoreError::NotFound { id })?;
    Ok(bar_from_row(&row)?)
}

#[cfg(test)]
#[path = "button_bars_tests.rs"]
mod tests;
