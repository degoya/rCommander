//! The `diagnostics.*` commands: the error log and the retention limits in `rcommander.db`
//! (RC-010-11).

use rc_db::{ErrorLogQuery, LogLevel, RetentionLimits, retained_table};

use crate::{Engine, EngineError, Outcome};

impl Engine {
    /// `diagnostics.error_log`: the stored lines, newest first. They were masked before they
    /// were stored.
    pub(crate) async fn error_log(
        &self,
        level: Option<LogLevel>,
        contains: Option<String>,
        limit: Option<u32>,
    ) -> Result<Outcome, EngineError> {
        let query = ErrorLogQuery {
            level,
            contains,
            limit,
        };
        let entries = self.db()?.error_log(&query).await?;
        Ok(Outcome::ErrorLog { entries })
    }

    /// `diagnostics.clear_error_log`.
    pub(crate) async fn clear_error_log(&self) -> Result<Outcome, EngineError> {
        let removed = self.db()?.clear_error_log().await?;
        Ok(Outcome::ErrorLogCleared { removed })
    }

    /// `diagnostics.get_retention`.
    pub(crate) async fn retention(&self) -> Result<Outcome, EngineError> {
        let settings = self.db()?.retention_settings().await?;
        Ok(Outcome::Retention {
            tables: settings.status(),
        })
    }

    /// `diagnostics.set_retention`: the given limits over the current ones, refused unless both
    /// lie within the table's bounds. The next sweep applies them.
    pub(crate) async fn set_retention(
        &self,
        table: &str,
        max_rows: Option<u64>,
        max_age_days: Option<u32>,
    ) -> Result<Outcome, EngineError> {
        let retained = retained_table(table)?;
        let db = self.db()?;
        let settings = db.retention_settings().await?;
        let current = settings.limits(table).unwrap_or(retained.defaults);
        let checked = retained.check(RetentionLimits {
            max_rows: max_rows.unwrap_or(current.max_rows),
            max_age_days: max_age_days.unwrap_or(current.max_age_days),
        })?;
        db.set_retention(checked).await?;
        Ok(Outcome::Retention {
            tables: settings.with(checked).status(),
        })
    }
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
