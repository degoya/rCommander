//! Own colour themes (RC-100-50, migration 0020): a set of colours a person made, kept under a
//! name that is unique without regard to case. The built-in themes are code in the window, never
//! rows here.
//!
//! The tokens are a JSON object the engine wrote and checked (`token -> "#rrggbb"`); this module
//! keeps the text and nothing more, and the engine checks what it reads back like any other
//! input. The table is durable ([`crate::DURABLE_TABLES`]): a person makes and removes themes.

use crate::{Db, Result};

/// One own theme as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredTheme {
    /// The engine's id, such as `own-1f2e3d4c`; it stays when the theme is renamed.
    pub id: String,
    pub name: String,
    /// The theme is for the dark mode; otherwise for the light one.
    pub dark: bool,
    /// Empty for the system's accent, else `#rrggbb`.
    pub accent: String,
    /// The JSON object `token -> "#rrggbb"`.
    pub tokens: String,
    /// When it was saved last, `strftime('%Y-%m-%dT%H:%M:%fZ')`.
    pub saved_at: String,
}

/// `id`, `name`, `dark`, `accent`, `tokens`, `saved_at`.
type Row = (String, String, i64, String, String, String);

fn from_row((id, name, dark, accent, tokens, saved_at): Row) -> StoredTheme {
    StoredTheme {
        id,
        name,
        dark: dark != 0,
        accent,
        tokens,
        saved_at,
    }
}

impl Db {
    /// Every own theme: the light ones first, each mode by name.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn themes(&self) -> Result<Vec<StoredTheme>> {
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id, name, dark, accent, tokens, saved_at FROM themes \
             ORDER BY dark, name COLLATE NOCASE",
        )
        .fetch_all(self.readers())
        .await?;
        Ok(rows.into_iter().map(from_row).collect())
    }

    /// The own theme `id`; `None` when there is none.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn theme(&self, id: &str) -> Result<Option<StoredTheme>> {
        let row: Option<Row> = sqlx::query_as(
            "SELECT id, name, dark, accent, tokens, saved_at FROM themes WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.readers())
        .await?;
        Ok(row.map(from_row))
    }

    /// Stores `theme` under its id, replacing the theme of that id; the theme as saved. The
    /// caller checks the name: another theme of the same name (case does not count) fails the
    /// table's unique constraint.
    ///
    /// # Errors
    ///
    /// A blank or taken name, tokens that are not JSON, or the write fails; nothing changes then.
    pub async fn save_theme(&self, theme: &StoredTheme) -> Result<StoredTheme> {
        let mut writer = self.writer().await;
        let row: Row = sqlx::query_as(
            "INSERT INTO themes (id, name, dark, accent, tokens) VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT (id) DO UPDATE SET name = excluded.name, dark = excluded.dark, \
             accent = excluded.accent, tokens = excluded.tokens, \
             saved_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
             RETURNING id, name, dark, accent, tokens, saved_at",
        )
        .bind(&theme.id)
        .bind(&theme.name)
        .bind(i64::from(theme.dark))
        .bind(&theme.accent)
        .bind(&theme.tokens)
        .fetch_one(&mut *writer)
        .await?;
        Ok(from_row(row))
    }

    /// Removes the own theme `id`; whether there was one.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn delete_theme(&self, id: &str) -> Result<bool> {
        let mut writer = self.writer().await;
        let deleted = sqlx::query("DELETE FROM themes WHERE id = ?")
            .bind(id)
            .execute(&mut *writer)
            .await?
            .rows_affected();
        Ok(deleted > 0)
    }
}

#[cfg(test)]
#[path = "themes_tests.rs"]
mod tests;
