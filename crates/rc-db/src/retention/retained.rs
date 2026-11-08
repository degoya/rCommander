//! Every table that grows with use, with its limits; split from `retention.rs` to keep it under
//! 500 lines (RC-080-66).

use super::{RetainedTable, RetentionLimits};

/// Every table that grows with use. A new one (history, the rename undo log, finished
/// transfers, the MCP audit log) adds its row here with its migration.
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
    // The most visited folders (RC-100-92): one row per folder with its halving count, for the
    // lower end of the history list (Alt+Down). A folder not visited for a year goes.
    RetainedTable {
        table: "dir_frequent",
        time_column: "last_visit",
        rows_key: "retention.dir_frequent.max_rows",
        age_key: "retention.dir_frequent.max_age_days",
        defaults: RetentionLimits {
            max_rows: 500,
            max_age_days: 365,
        },
        floor: RetentionLimits {
            max_rows: 50,
            max_age_days: 30,
        },
        ceiling: RetentionLimits {
            max_rows: 5_000,
            max_age_days: 3_650,
        },
    },
    // The MCP audit log (RC-080-66): one row per tool call the gate decided - masked arguments
    // cut to 2 KiB, never an answer. An agent's busy hour is some hundred calls.
    RetainedTable {
        table: "mcp_audit_log",
        time_column: "recorded_at",
        rows_key: "retention.mcp_audit_log.max_rows",
        age_key: "retention.mcp_audit_log.max_age_days",
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
];
