//! Which MCP tool covers each engine command, or why none does.
//!
//! Every row of `rc_engine::COMMANDS` has exactly one row here, kept with its registry group:
//! `coverage/<group>.rs` holds the rows of `registry/<group>.rs`, sorted by id, and `COVERAGE`
//! is all of them in the registry's order of groups (`rc_engine::command_groups!`).
//! `tests/mcp_coverage.rs` fails when one is missing in either direction or a `Covered` tool is
//! not served. The rows are also the source of `crates/rc-mcp/mcp-coverage.md`:
//! `scripts/mcp-coverage.sh` reads every file of `coverage/` without compiling, so a row keeps
//! this exact shape - `("<id>", Coverage::Covered("<tool>")),` or
//! `("<id>", Coverage::Omitted("<reason>")),`, with no escaped quotes in the reason - inside a
//! `const ROWS` table.

mod agent;
mod app;
mod archive;
mod archive_ops;
mod backup;
mod certificates;
mod checksum;
mod command_line;
mod compare;
mod connections;
mod diagnostics;
mod drives;
mod file;
mod file_ops;
mod history;
mod hostkeys;
mod hotlist;
mod import;
mod login_scripts;
mod manager;
mod mcp;
mod multi_rename;
mod ops;
mod panel;
mod plugins;
mod properties;
mod queue;
mod remote;
mod render;
mod search;
mod secrets;
mod sessions;
mod settings;
mod ssh_exec;
mod ssh_terminal;
mod tabs;
mod terminal;
mod theme;
mod update;
mod vault;
mod viewer;
mod wsl;

pub use render::{command_of, render_markdown};

/// How a command reaches MCP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    /// The tool of this name wraps the command.
    Covered(&'static str),
    /// No tool, for this reason.
    Omitted(&'static str),
}

/// Writes `GROUPS` from the registry's one list of groups.
macro_rules! coverage_groups {
    ([$($group:ident,)*]) => {
        /// Each group's rows, in the order of the registry's groups.
        const GROUPS: &[&[(&str, Coverage)]] = &[$($group::ROWS,)*];
    };
}

rc_engine::command_groups!(coverage_groups);

/// One row per command: the groups' rows one after the other.
pub const COVERAGE: &[(&str, Coverage)] = &ALL;

const ALL: [(&str, Coverage); count(GROUPS)] = joined(GROUPS);

/// How many rows `groups` hold together.
const fn count(groups: &[&[(&str, Coverage)]]) -> usize {
    let (mut total, mut group) = (0, 0);
    while group < groups.len() {
        total += groups[group].len();
        group += 1;
    }
    total
}

/// The rows of `groups` one after the other; `N` is their [`count`].
const fn joined<const N: usize>(
    groups: &[&[(&'static str, Coverage)]],
) -> [(&'static str, Coverage); N] {
    let mut rows = [("", Coverage::Omitted("")); N];
    let (mut at, mut group) = (0, 0);
    while group < groups.len() {
        let mut row = 0;
        while row < groups[group].len() {
            rows[at] = groups[group][row];
            at += 1;
            row += 1;
        }
        group += 1;
    }
    rows
}
