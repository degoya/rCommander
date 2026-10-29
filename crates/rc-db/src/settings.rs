//! Settings: one JSON value per key, typed at the call.

use std::collections::BTreeMap;

use serde::{Serialize, de::DeserializeOwned};
use sqlx::{AssertSqlSafe, SqliteConnection};

use crate::{Db, DbError, Result};

impl Db {
    /// The setting `key` as `T`, or `None` when it was never set.
    ///
    /// # Errors
    ///
    /// The query fails, or the stored JSON does not deserialize into `T` ([`DbError::Json`]).
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let raw: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(self.readers())
            .await?;
        raw.map(|text| {
            serde_json::from_str(&text).map_err(|source| DbError::Json {
                key: key.to_owned(),
                source,
            })
        })
        .transpose()
    }

    /// The stored JSON text of each of `keys` that is set, in one query: a caller that needs
    /// several settings per call (the MCP gate, RC-080-54) reads them at once and decodes each
    /// as it would [`Db::get`]'s, so one bad value spoils only its own key.
    ///
    /// # Errors
    ///
    /// The query fails.
    pub async fn get_json_many(&self, keys: &[&str]) -> Result<BTreeMap<String, String>> {
        if keys.is_empty() {
            return Ok(BTreeMap::new());
        }
        // Only placeholders are formatted in; the keys are bound.
        let sql = format!(
            "SELECT key, value FROM settings WHERE key IN ({})",
            vec!["?"; keys.len()].join(", ")
        );
        let mut query = sqlx::query_as::<_, (String, String)>(AssertSqlSafe(sql));
        for key in keys {
            query = query.bind(*key);
        }
        Ok(query.fetch_all(self.readers()).await?.into_iter().collect())
    }

    /// Stores `value` under `key`, replacing what was there.
    ///
    /// # Errors
    ///
    /// `value` does not serialize ([`DbError::Json`]), or the write fails.
    pub async fn set<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> Result<()> {
        let text = serde_json::to_string(value).map_err(|source| DbError::Json {
            key: key.to_owned(),
            source,
        })?;
        let mut writer = self.writer().await;
        write_setting(&mut writer, key, &text).await
    }

    /// Removes the setting `key`; says whether there was one.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn delete(&self, key: &str) -> Result<bool> {
        let mut writer = self.writer().await;
        let done = sqlx::query("DELETE FROM settings WHERE key = ?")
            .bind(key)
            .execute(&mut *writer)
            .await?;
        Ok(done.rows_affected() > 0)
    }
}

/// The raw JSON of the setting `key`, read on `connection` (inside a write's transaction).
pub(crate) async fn read_setting(
    connection: &mut SqliteConnection,
    key: &str,
) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&mut *connection)
            .await?,
    )
}

/// Stores the JSON `text` under `key` on `connection`, replacing what was there.
pub(crate) async fn write_setting(
    connection: &mut SqliteConnection,
    key: &str,
    text: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?) \
         ON CONFLICT (key) DO UPDATE SET value = excluded.value, \
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
    )
    .bind(key)
    .bind(text)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
