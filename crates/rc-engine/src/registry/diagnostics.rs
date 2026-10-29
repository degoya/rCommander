//! The error log and the retention limits (RC-010-11).

use serde::{Deserialize, Serialize};

use super::{CommandClass, CommandSpec};
use crate::LogLevel;

/// The parameters of `diagnostics.error_log`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorLogRequest {
    /// Only lines of this level.
    #[serde(default)]
    pub level: Option<LogLevel>,
    /// Only lines whose message or target contains this, ignoring ASCII case.
    #[serde(default)]
    pub contains: Option<String>,
    /// Lines at most; `rc_db::ERROR_LOG_PAGE` when omitted.
    #[serde(default)]
    pub limit: Option<u32>,
}

/// The parameters of `diagnostics.set_retention`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionRequest {
    /// A table of `rc_db::RETAINED_TABLES`.
    pub table: String,
    /// The new row limit; unchanged when omitted.
    #[serde(default)]
    pub max_rows: Option<u64>,
    /// The new age limit in days; unchanged when omitted.
    #[serde(default)]
    pub max_age_days: Option<u32>,
}

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "diagnostics.clear_error_log",
        class: CommandClass::Destructive,
        summary: "Delete every line of the error log",
    },
    CommandSpec {
        id: "diagnostics.error_log",
        class: CommandClass::Read,
        summary: "The stored warn and error lines, newest first, masked; by level or text",
    },
    CommandSpec {
        id: "diagnostics.get_retention",
        class: CommandClass::Read,
        summary: "The row and age limits of each growing table, with defaults and bounds",
    },
    CommandSpec {
        id: "diagnostics.set_retention",
        class: CommandClass::Write,
        summary: "Change the row or age limit of a growing table, within its bounds",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "diagnostics.error_log" => ErrorLog(ErrorLogRequest),
            "diagnostics.clear_error_log" => ClearErrorLog,
            "diagnostics.get_retention" => GetRetention,
            "diagnostics.set_retention" => SetRetention(RetentionRequest),
        } }
    };
}
pub(super) use commands;
