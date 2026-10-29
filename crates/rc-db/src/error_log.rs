//! The error log in `rcommander.db` (RC-010-11): `warn` and `error` lines only, masked and
//! capped on the way in ([`layer`]), kept within its limits by the retention sweep
//! (`RETAINED_TABLES`), read and cleared through the `diagnostics.*` commands.

pub(crate) mod layer;

use serde::{Deserialize, Serialize};
use sqlx::Connection as _;

use crate::{Db, Result};

/// Lines one read answers when no limit is given.
pub const ERROR_LOG_PAGE: u32 = 100;

/// Lines one read answers at most.
pub const ERROR_LOG_MAX_PAGE: u32 = 1_000;

/// How bad a stored line is. `info` and `debug` lines never reach the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Warn,
    Error,
}

impl LogLevel {
    /// The level as the table stores it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    /// The stored text read back; the table's `CHECK` admits nothing but the two.
    fn from_stored(text: &str) -> Self {
        if text == "error" {
            Self::Error
        } else {
            Self::Warn
        }
    }
}

/// A line on its way into the table, already masked and capped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewErrorLogEntry {
    pub(crate) level: LogLevel,
    pub(crate) target: String,
    pub(crate) message: String,
    pub(crate) code: Option<String>,
}

/// A stored line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorLogEntry {
    pub id: i64,
    /// UTC, `2026-10-17T09:30:00.123Z`.
    pub recorded_at: String,
    pub level: LogLevel,
    /// The module that logged it (`rc_mcp::server`).
    pub target: String,
    pub message: String,
    /// The stable error code, when the line carried one (`code = "not_found"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// Which lines a read answers, newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ErrorLogQuery {
    /// Only lines of this level.
    pub level: Option<LogLevel>,
    /// Only lines whose message or target contains this, ignoring ASCII case.
    pub contains: Option<String>,
    /// Lines at most: [`ERROR_LOG_PAGE`] when `None`, never more than [`ERROR_LOG_MAX_PAGE`].
    pub limit: Option<u32>,
}

type Row = (i64, String, String, String, String, Option<String>);

impl Db {
    /// The stored lines `query` asks for, newest first.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn error_log(&self, query: &ErrorLogQuery) -> Result<Vec<ErrorLogEntry>> {
        let limit = query
            .limit
            .unwrap_or(ERROR_LOG_PAGE)
            .clamp(1, ERROR_LOG_MAX_PAGE);
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id, recorded_at, level, target, message, code FROM error_log \
             WHERE (?1 IS NULL OR level = ?1) \
               AND (?2 IS NULL OR instr(lower(message), lower(?2)) > 0 \
                    OR instr(lower(target), lower(?2)) > 0) \
             ORDER BY id DESC LIMIT ?3",
        )
        .bind(query.level.map(LogLevel::as_str))
        .bind(query.contains.as_deref())
        .bind(i64::from(limit))
        .fetch_all(self.readers())
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, recorded_at, level, target, message, code)| ErrorLogEntry {
                    id,
                    recorded_at,
                    level: LogLevel::from_stored(&level),
                    target,
                    message,
                    code,
                },
            )
            .collect())
    }

    /// Deletes every stored line and gives the space back; how many there were.
    ///
    /// # Errors
    ///
    /// The delete fails.
    pub async fn clear_error_log(&self) -> Result<u64> {
        let removed = {
            let mut writer = self.writer().await;
            sqlx::query("DELETE FROM error_log")
                .execute(&mut *writer)
                .await?
                .rows_affected()
        };
        if removed > 0 {
            self.reclaim().await?;
        }
        Ok(removed)
    }

    /// Stores `entries` in one transaction; how many.
    pub(crate) async fn append_error_log(&self, entries: &[NewErrorLogEntry]) -> Result<u64> {
        if entries.is_empty() {
            return Ok(0);
        }
        let mut writer = self.writer().await;
        let mut transaction = writer.begin().await?;
        for entry in entries {
            sqlx::query("INSERT INTO error_log (level, target, message, code) VALUES (?, ?, ?, ?)")
                .bind(entry.level.as_str())
                .bind(&entry.target)
                .bind(&entry.message)
                .bind(entry.code.as_deref())
                .execute(&mut *transaction)
                .await?;
        }
        transaction.commit().await?;
        Ok(u64::try_from(entries.len()).unwrap_or(u64::MAX))
    }
}

#[cfg(test)]
#[path = "error_log_tests.rs"]
mod tests;
