//! The colour themes of the connections (RC-100-51), read on their own for the window: which
//! connection's panel and SSH terminal draw with which theme in the light and in the dark mode.
//! A deleted own theme reads as `None` here as in every connection ([`theme_column`]).

use serde::{Deserialize, Serialize};
use sqlx::Row as _;

use crate::{
    Db,
    connections::{ConnectionId, TreeResult, theme_column},
};

/// One connection's themes; at least one of them is set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionThemes {
    pub id: ConnectionId,
    pub light: Option<String>,
    pub dark: Option<String>,
}

impl Db {
    /// Every connection with a theme of its own, by id.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn connection_themes(&self) -> TreeResult<Vec<ConnectionThemes>> {
        let rows = sqlx::query(concat!(
            "SELECT id, ",
            theme_column!("theme_light"),
            ", ",
            theme_column!("theme_dark"),
            " FROM connections WHERE theme_light IS NOT NULL OR theme_dark IS NOT NULL ORDER BY id"
        ))
        .fetch_all(self.readers())
        .await?;
        let mut themes = Vec::with_capacity(rows.len());
        for row in rows {
            let light: Option<String> = row.try_get("theme_light")?;
            let dark: Option<String> = row.try_get("theme_dark")?;
            if light.is_some() || dark.is_some() {
                themes.push(ConnectionThemes {
                    id: ConnectionId(row.try_get("id")?),
                    light,
                    dark,
                });
            }
        }
        Ok(themes)
    }
}

#[cfg(test)]
#[path = "themes_tests.rs"]
mod tests;
