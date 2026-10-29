//! Storage discipline (RC-010-11): the database holds durable state, and every table that grows
//! with use has a row and an age limit.
//!
//! [`RETAINED_TABLES`] names each growing table with its limits; [`DURABLE_TABLES`] names every
//! other table with the reason it may keep what it holds. `retention_tests` fails when a table
//! of the migrations is in neither list, so a new table cannot grow unnoticed. [`Db::prune`]
//! (`retention/prune.rs`) keeps the retained tables within their limits.
//!
//! The limits are settings (`retention.<table>.max_rows`, `retention.<table>.max_age_days`),
//! read anew on every sweep. A person can lower them, or raise them up to a ceiling, but never
//! switch one off: each lies between the table's floor and ceiling.

mod prune;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Db, Result};

pub use prune::{PRUNE_BATCH, PruneReport};

/// A row and an age limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionLimits {
    /// Rows kept at most; the oldest go first.
    pub max_rows: u64,
    /// Days a row is kept at most.
    pub max_age_days: u32,
}

/// A table that grows with use, and how far it may.
#[derive(Debug, PartialEq, Eq)]
pub struct RetainedTable {
    /// The table. It has `id INTEGER PRIMARY KEY` in insertion order, which the sweep deletes by.
    pub table: &'static str,
    /// The column with each row's time, as `strftime('%Y-%m-%dT%H:%M:%fZ')` text.
    pub time_column: &'static str,
    /// The setting with the row limit.
    pub rows_key: &'static str,
    /// The setting with the age limit.
    pub age_key: &'static str,
    /// The limits while nothing is set.
    pub defaults: RetentionLimits,
    /// The lowest limits a setting may give.
    pub floor: RetentionLimits,
    /// The highest limits a setting may give: there is no "keep forever".
    pub ceiling: RetentionLimits,
}

/// Every table that grows with use. A new one (history, the rename undo log, finished
/// transfers, an MCP audit) adds its row here with its migration.
pub const RETAINED_TABLES: &[RetainedTable] = &[
    // The warn and error lines of the tracing layer (RC-010-11).
    RetainedTable {
        table: "error_log",
        time_column: "recorded_at",
        rows_key: "retention.error_log.max_rows",
        age_key: "retention.error_log.max_age_days",
        defaults: RetentionLimits {
            max_rows: 5_000,
            max_age_days: 30,
        },
        floor: RetentionLimits {
            max_rows: 100,
            max_age_days: 1,
        },
        ceiling: RetentionLimits {
            max_rows: 50_000,
            max_age_days: 365,
        },
    },
    // The multi-rename undo log (RC-020-01): one row per step. The defaults are
    // `rc_rename::UNDO_LOG_MAX_STEPS` and `UNDO_LOG_MAX_AGE_DAYS`; an undo reverses the latest run
    // only, so the floor still holds a large one.
    RetainedTable {
        table: "rename_log",
        time_column: "recorded_at",
        rows_key: "retention.rename_log.max_rows",
        age_key: "retention.rename_log.max_age_days",
        defaults: RetentionLimits {
            max_rows: 100_000,
            max_age_days: 30,
        },
        floor: RetentionLimits {
            max_rows: 1_000,
            max_age_days: 1,
        },
        ceiling: RetentionLimits {
            max_rows: 1_000_000,
            max_age_days: 365,
        },
    },
    // The transfer jobs to and from servers that have not ended (RC-060-07): a row per job while it
    // runs, gone when it ends; what stays is a job the app left in the middle, which the next
    // start puts back to resume. One nobody resumes is dropped after a month.
    RetainedTable {
        table: "transfer_jobs",
        time_column: "recorded_at",
        rows_key: "retention.transfer_jobs.max_rows",
        age_key: "retention.transfer_jobs.max_age_days",
        defaults: RetentionLimits {
            max_rows: 200,
            max_age_days: 30,
        },
        floor: RetentionLimits {
            max_rows: 10,
            max_age_days: 1,
        },
        ceiling: RetentionLimits {
            max_rows: 5_000,
            max_age_days: 365,
        },
    },
    // Every plugin call (RC-100-09): plugin, version, export, outcome, code, duration - codes,
    // never a plugin's words. A viewer in Quick View calls on every cursor move, so the default
    // keeps a busy day.
    RetainedTable {
        table: "plugin_executions",
        time_column: "recorded_at",
        rows_key: "retention.plugin_executions.max_rows",
        age_key: "retention.plugin_executions.max_age_days",
        defaults: RetentionLimits {
            max_rows: 10_000,
            max_age_days: 30,
        },
        floor: RetentionLimits {
            max_rows: 100,
            max_age_days: 1,
        },
        ceiling: RetentionLimits {
            max_rows: 100_000,
            max_age_days: 365,
        },
    },
    // The panels' folder history (RC-010-03).
    RetainedTable {
        table: "dir_history",
        time_column: "saved_at",
        rows_key: "retention.dir_history.max_rows",
        age_key: "retention.dir_history.max_age_days",
        defaults: RetentionLimits {
            max_rows: 2_000,
            max_age_days: 90,
        },
        floor: RetentionLimits {
            max_rows: 100,
            max_age_days: 1,
        },
        ceiling: RetentionLimits {
            max_rows: 20_000,
            max_age_days: 365,
        },
    },
];

/// Every table that keeps what it holds, with the reason it does not grow with use.
pub const DURABLE_TABLES: &[(&str, &str)] = &[
    (
        "_sqlx_migrations",
        "sqlx's ledger of applied migrations: one row per migration file",
    ),
    (
        "connection_folders",
        "one row per folder of the connection tree the person made",
    ),
    (
        "connection_session_prefs",
        "at most one row per connection: its SSH session settings, replaced when changed and \
         deleted with the connection",
    ),
    (
        "connections",
        "one row per saved connection, with its encrypted secrets (0006) and the source an \
         import read it from (0007); a person or an import adds them, a person removes them",
    ),
    (
        "credentials",
        "one row per shared credential the person made (0018), with its encrypted secrets; a \
         person adds and removes them",
    ),
    (
        "hotlist",
        "one row per hotlist entry or folder a person made; a person removes them",
    ),
    (
        "known_hosts",
        "one row per SSH host key a person trusted; replaced, never appended",
    ),
    (
        "login_script_secrets",
        "one row per secret a person stored with a login script; deleted with its script",
    ),
    (
        "login_scripts",
        "one row per login script a person made; a person removes them",
    ),
    (
        "plugin_digest_revocations",
        "one row per package version a person withdrew by its digest; never appended by use",
    ),
    (
        "plugin_keys",
        "one row per plugin signing key a person confirmed or revoked; a key id stays pinned",
    ),
    (
        "plugins",
        "one row per installed plugin: its switch and version choices, removed with the plugin",
    ),
    (
        "saved_queue_entries",
        "the jobs of a saved transfer list, in their order; replaced when the list is saved again \
         and deleted with it - finished jobs are never recorded",
    ),
    (
        "saved_queues",
        "one row per transfer list a person saved under a name; a person removes them",
    ),
    (
        "secret_sources",
        "at most one row per connection and secret kind: the source a person mapped it to, never \
         a value; replaced when changed and deleted with the connection",
    ),
    (
        "settings",
        "one row per setting key; a key is replaced, never appended",
    ),
    (
        "tabs",
        "one row per open folder tab of the two panels, rewritten whole when the app ends",
    ),
    (
        "tls_certificates",
        "at most one row per FTPS host and port: the certificate a person trusted; removed by a \
         person, never appended",
    ),
    (
        "vault_meta",
        "at most one row: the vault's key-derivation parameters and verifier",
    ),
];

/// The retained table named `table`.
///
/// # Errors
///
/// [`RetentionError::UnknownTable`] for any other name.
pub fn retained_table(table: &str) -> Result<&'static RetainedTable, RetentionError> {
    RETAINED_TABLES
        .iter()
        .find(|retained| retained.table == table)
        .ok_or_else(|| RetentionError::UnknownTable {
            table: table.to_owned(),
        })
}

/// A retention change that cannot be made.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RetentionError {
    /// The table has no retention limits.
    #[error("{table} is not a table with retention limits")]
    UnknownTable { table: String },
    /// A limit outside the table's floor and ceiling.
    #[error("{key} must be between {min} and {max}, not {value}")]
    OutOfRange {
        key: &'static str,
        value: u64,
        min: u64,
        max: u64,
    },
}

/// Limits a [`RetainedTable::check`] accepted; the only thing [`Db::set_retention`] stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckedLimits {
    table: &'static RetainedTable,
    limits: RetentionLimits,
}

impl CheckedLimits {
    /// The table the limits are for.
    #[must_use]
    pub fn table(&self) -> &'static RetainedTable {
        self.table
    }

    /// The limits.
    #[must_use]
    pub fn limits(&self) -> RetentionLimits {
        self.limits
    }
}

impl RetainedTable {
    /// `limits`, when both lie between the floor and the ceiling.
    ///
    /// # Errors
    ///
    /// [`RetentionError::OutOfRange`] naming the first limit that does not.
    pub fn check(&'static self, limits: RetentionLimits) -> Result<CheckedLimits, RetentionError> {
        in_range(
            self.rows_key,
            limits.max_rows,
            self.floor.max_rows,
            self.ceiling.max_rows,
        )?;
        in_range(
            self.age_key,
            u64::from(limits.max_age_days),
            u64::from(self.floor.max_age_days),
            u64::from(self.ceiling.max_age_days),
        )?;
        Ok(CheckedLimits {
            table: self,
            limits,
        })
    }

    /// `limits` moved between the floor and the ceiling: what a hand-edited setting becomes.
    #[must_use]
    pub fn clamp(&self, limits: RetentionLimits) -> RetentionLimits {
        RetentionLimits {
            max_rows: limits
                .max_rows
                .clamp(self.floor.max_rows, self.ceiling.max_rows),
            max_age_days: limits
                .max_age_days
                .clamp(self.floor.max_age_days, self.ceiling.max_age_days),
        }
    }
}

fn in_range(key: &'static str, value: u64, min: u64, max: u64) -> Result<(), RetentionError> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(RetentionError::OutOfRange {
            key,
            value,
            min,
            max,
        })
    }
}

/// The limits of every retained table, as one sweep applies them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionSettings {
    limits: Vec<(&'static RetainedTable, RetentionLimits)>,
}

impl RetentionSettings {
    /// Every table at its defaults.
    #[must_use]
    pub fn defaults() -> Self {
        Self {
            limits: RETAINED_TABLES
                .iter()
                .map(|table| (table, table.defaults))
                .collect(),
        }
    }

    /// The limits of `table`.
    #[must_use]
    pub fn limits(&self, table: &str) -> Option<RetentionLimits> {
        self.iter()
            .find(|(retained, _)| retained.table == table)
            .map(|(_, limits)| limits)
    }

    /// These settings with `checked` in place of its table's limits.
    #[must_use]
    pub fn with(mut self, checked: CheckedLimits) -> Self {
        for (table, limits) in &mut self.limits {
            if table.table == checked.table.table {
                *limits = checked.limits;
            }
        }
        self
    }

    /// Each table with its limits.
    pub fn iter(&self) -> impl Iterator<Item = (&'static RetainedTable, RetentionLimits)> + '_ {
        self.limits.iter().copied()
    }

    /// Each table with its limits, defaults and bounds, as the `diagnostics.*` commands answer.
    #[must_use]
    pub fn status(&self) -> Vec<RetentionStatus> {
        self.iter()
            .map(|(table, limits)| RetentionStatus {
                table: table.table.to_owned(),
                limits,
                defaults: table.defaults,
                floor: table.floor,
                ceiling: table.ceiling,
            })
            .collect()
    }
}

/// One retained table's limits as a command reports them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionStatus {
    pub table: String,
    /// What the sweep applies now.
    pub limits: RetentionLimits,
    pub defaults: RetentionLimits,
    /// The lowest limits a setting may give.
    pub floor: RetentionLimits,
    /// The highest limits a setting may give.
    pub ceiling: RetentionLimits,
}

impl Db {
    /// The limits the sweep applies now: the stored settings, each moved between its table's
    /// floor and ceiling; the default where nothing (or no number) is stored.
    ///
    /// # Errors
    ///
    /// A settings read fails.
    pub async fn retention_settings(&self) -> Result<RetentionSettings> {
        // Every table's two keys in one query, not two per table (RC-080-54).
        let stored: BTreeMap<String, String> =
            sqlx::query_as("SELECT key, value FROM settings WHERE key LIKE 'retention.%'")
                .fetch_all(self.readers())
                .await?
                .into_iter()
                .collect();
        let mut limits = Vec::with_capacity(RETAINED_TABLES.len());
        for table in RETAINED_TABLES {
            let rows = stored_limit(&stored, table.rows_key);
            let age = stored_limit(&stored, table.age_key);
            let stored = RetentionLimits {
                max_rows: rows.unwrap_or(table.defaults.max_rows),
                max_age_days: age.map_or(table.defaults.max_age_days, |days| {
                    u32::try_from(days).unwrap_or(u32::MAX)
                }),
            };
            limits.push((table, table.clamp(stored)));
        }
        Ok(RetentionSettings { limits })
    }

    /// Stores checked limits; the next sweep applies them.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn set_retention(&self, checked: CheckedLimits) -> Result<()> {
        self.set(checked.table.rows_key, &checked.limits.max_rows)
            .await?;
        self.set(checked.table.age_key, &checked.limits.max_age_days)
            .await
    }
}

/// One stored limit; `None` when absent or not a whole number, so a bad value can never stop
/// the sweep.
fn stored_limit(stored: &BTreeMap<String, String>, key: &str) -> Option<u64> {
    let text = stored.get(key)?;
    match serde_json::from_str::<u64>(text) {
        Ok(value) => Some(value),
        Err(_) => {
            tracing::warn!(
                key,
                "a retention setting is not a whole number; its default applies"
            );
            None
        }
    }
}

#[cfg(test)]
#[path = "retention_tests.rs"]
mod tests;
