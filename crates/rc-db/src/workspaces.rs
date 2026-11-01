//! Workspaces (RC-100-68, migration 0022): what both panels show, saved under a name that is
//! unique without regard to case, in the order the person put them.
//!
//! The spec is a JSON object the engine wrote and checked (`rc_engine::workspaces::WorkspaceSpec`:
//! folders, connection ids, shell names, never a secret); this module keeps the text and nothing
//! more, and the engine checks what it reads back like any other input. The table is durable
//! ([`crate::DURABLE_TABLES`]): a person makes and removes workspaces.

use sqlx::{Connection as _, SqliteConnection};

use crate::{Db, Result};

/// One workspace as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredWorkspace {
    pub id: i64,
    pub name: String,
    /// Its place in the list, smallest first.
    pub sort_order: i64,
    /// The engine's JSON.
    pub spec: String,
    /// The theme ids of the light and the dark mode; `None` leaves the window's.
    pub theme_light: Option<String>,
    pub theme_dark: Option<String>,
    /// `strftime('%Y-%m-%dT%H:%M:%fZ')`.
    pub created_at: String,
    pub updated_at: String,
}

/// A workspace to store: a new one, or the changes to a stored one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspacePatch {
    pub name: Option<String>,
    pub spec: Option<String>,
    /// `Some(None)` clears the theme, `None` leaves it.
    pub theme_light: Option<Option<String>>,
    pub theme_dark: Option<Option<String>>,
}

/// `id`, `name`, `sort_order`, `spec`, `theme_light`, `theme_dark`, `created_at`, `updated_at`.
type Row = (
    i64,
    String,
    i64,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
);

/// The columns of a workspace; literal, so every query stays a literal.
macro_rules! columns {
    () => {
        "id, name, sort_order, spec, theme_light, theme_dark, created_at, updated_at"
    };
}

fn from_row(row: Row) -> StoredWorkspace {
    let (id, name, sort_order, spec, theme_light, theme_dark, created_at, updated_at) = row;
    StoredWorkspace {
        id,
        name,
        sort_order,
        spec,
        theme_light,
        theme_dark,
        created_at,
        updated_at,
    }
}

impl Db {
    /// Every workspace in the person's order.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn workspaces(&self) -> Result<Vec<StoredWorkspace>> {
        let rows: Vec<Row> = sqlx::query_as(concat!(
            "SELECT ",
            columns!(),
            " FROM workspaces ORDER BY sort_order, id"
        ))
        .fetch_all(self.readers())
        .await?;
        Ok(rows.into_iter().map(from_row).collect())
    }

    /// The workspace `id`; `None` when there is none.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn workspace(&self, id: i64) -> Result<Option<StoredWorkspace>> {
        let row: Option<Row> = sqlx::query_as(concat!(
            "SELECT ",
            columns!(),
            " FROM workspaces WHERE id = ?"
        ))
        .bind(id)
        .fetch_optional(self.readers())
        .await?;
        Ok(row.map(from_row))
    }

    /// Stores a new workspace after the last one; the workspace as stored. The caller checks
    /// the name: one another workspace has (case does not count) fails the unique constraint.
    ///
    /// # Errors
    ///
    /// A blank or taken name, a spec that is not JSON, or the write fails; nothing is stored.
    pub async fn create_workspace(
        &self,
        name: &str,
        spec: &str,
        theme_light: Option<&str>,
        theme_dark: Option<&str>,
    ) -> Result<StoredWorkspace> {
        let mut writer = self.writer().await;
        let row: Row = sqlx::query_as(concat!(
            "INSERT INTO workspaces (name, sort_order, spec, theme_light, theme_dark) VALUES \
             (?, (SELECT ifnull(max(sort_order) + 1, 0) FROM workspaces), ?, ?, ?) RETURNING ",
            columns!()
        ))
        .bind(name)
        .bind(spec)
        .bind(theme_light)
        .bind(theme_dark)
        .fetch_one(&mut *writer)
        .await?;
        Ok(from_row(row))
    }

    /// Changes what `patch` names of the workspace `id`; the workspace as stored now, `None`
    /// when there is none.
    ///
    /// # Errors
    ///
    /// A blank or taken name, a spec that is not JSON, or the write fails; nothing changes.
    pub async fn update_workspace(
        &self,
        id: i64,
        patch: &WorkspacePatch,
    ) -> Result<Option<StoredWorkspace>> {
        let mut writer = self.writer().await;
        let row: Option<Row> = sqlx::query_as(concat!(
            "UPDATE workspaces SET name = coalesce(?, name), spec = coalesce(?, spec), \
             theme_light = CASE WHEN ? THEN ? ELSE theme_light END, \
             theme_dark = CASE WHEN ? THEN ? ELSE theme_dark END, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ? RETURNING ",
            columns!()
        ))
        .bind(patch.name.as_deref())
        .bind(patch.spec.as_deref())
        .bind(patch.theme_light.is_some())
        .bind(patch.theme_light.clone().flatten())
        .bind(patch.theme_dark.is_some())
        .bind(patch.theme_dark.clone().flatten())
        .bind(id)
        .fetch_optional(&mut *writer)
        .await?;
        Ok(row.map(from_row))
    }

    /// Puts the workspace `id` at `position` (from 0, the last place when beyond it) and numbers
    /// the list anew, in one transaction; whether there was such a workspace.
    ///
    /// # Errors
    ///
    /// The write fails; the order stays as it was.
    pub async fn move_workspace(&self, id: i64, position: usize) -> Result<bool> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let mut order: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM workspaces ORDER BY sort_order, id")
                .fetch_all(&mut *tx)
                .await?;
        let Some(from) = order.iter().position(|&known| known == id) else {
            return Ok(false);
        };
        let moved = order.remove(from);
        order.insert(position.min(order.len()), moved);
        renumber(&mut tx, &order).await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Removes the workspace `id`; whether there was one.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn delete_workspace(&self, id: i64) -> Result<bool> {
        let mut writer = self.writer().await;
        let deleted = sqlx::query("DELETE FROM workspaces WHERE id = ?")
            .bind(id)
            .execute(&mut *writer)
            .await?
            .rows_affected();
        Ok(deleted > 0)
    }
}

/// Gives the workspaces of `order` the places 0, 1, 2, ... in that order.
async fn renumber(connection: &mut SqliteConnection, order: &[i64]) -> Result<()> {
    for (place, id) in order.iter().enumerate() {
        sqlx::query("UPDATE workspaces SET sort_order = ? WHERE id = ?")
            .bind(i64::try_from(place).unwrap_or(i64::MAX))
            .bind(*id)
            .execute(&mut *connection)
            .await?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "workspaces_tests.rs"]
mod tests;
