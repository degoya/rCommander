//! The command registry: every user action is one [`Command`], named by a stable id.
//!
//! Total Commander names its actions `cm_*` and binds keys, menus and buttons to those names;
//! rCommander does the same with ids like `panel.navigate`. The keymap, the menus and the MCP
//! server all turn their input into a [`Command`] and hand it to [`crate::Engine::dispatch`].
//!
//! A new command is two additions to its group's module (`registry/<group>.rs`, one module per
//! group): a variant of [`Command`] in the group's `commands!` list, after its id (its
//! parameters are the variant's fields; `command.rs` builds the enum, the serde names and
//! [`Command::id`] from every group's list), and a row with the same id, its class and a
//! one-line summary in the group's table, assembled into [`COMMANDS`]. Its arm of
//! [`crate::Engine::dispatch`] goes into the group's `dispatch/arms/<group>.rs`; without it the
//! crate does not compile. A new group goes into [`crate::command_groups!`], the one list the
//! enum, `GROUPS`, the dispatch `match` and `rc-mcp`'s coverage are built from. `registry_tests`
//! holds them against each other, and `rc-mcp`'s coverage test fails until the command has an
//! MCP tool or a reason why not.

use std::{collections::HashMap, sync::LazyLock};

mod command;
mod escalation;
mod vocabulary;

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
mod search;
mod secrets;
mod sessions;
mod settings;
mod ssh_exec;
mod ssh_terminal;
mod tabs;
mod terminal;
mod update;
mod vault;
mod viewer;
mod wsl;

pub use backup::BackupToRequest;
pub use checksum::{ChecksumDialog, ChecksumFilesRequest, ChecksumJob, ChecksumRequest};
pub use command::Command;
pub use connections::{
    CredentialFields, MoveConnectionRequest, MoveFolderRequest, SaveConnectionRequest,
    SaveCredentialRequest, SaveFields, SecretChange,
};
pub use diagnostics::{ErrorLogRequest, RetentionRequest};
pub(crate) use escalation::overwrites;
pub use escalation::{ESCALATIONS, Escalation, escalation};
pub use file::{CombineRequest, SplitRequest};
pub use import::{
    ConfigExportRequest, ConfigImportRequest, RdmImportRequest, SshConfigImportRequest,
    TcImportRequest, TcPasswordsRequest,
};
pub use ops::{DeleteRequest, TransferRequest};
pub use panel::MaskRequest;
pub use remote::QuickConnectRequest;
pub use search::SearchRequest;
pub use secrets::ImportKeyRequest;
pub use ssh_exec::SshExecRequest;
pub use vocabulary::{CommandClass, CommandSpec, CursorStep, CursorTarget};

/// Every command group, once and in the order of [`COMMANDS`] (RC-080-48): a module of
/// `registry/` each (its `COMMANDS` table and its `commands!` list of [`Command`] variants), of
/// `dispatch/arms/` (its `arms!`) and of `rc-mcp`'s `coverage/` (its `ROWS`). It hands the
/// groups to `$callback!`, after `$args`: `GROUPS` below, the [`Command`] enum (`command.rs`),
/// the `match` of [`crate::Engine::dispatch`] and `rc-mcp`'s coverage table are all built from
/// this one list. `scripts/mcp-coverage.sh` reads it from the source, so it keeps this shape:
/// one group per line, each followed by a comma.
#[doc(hidden)]
#[macro_export]
macro_rules! command_groups {
    ($callback:ident $(, $args:tt)*) => {
        $callback! { $($args,)* [
            agent,
            archive,
            archive_ops,
            backup,
            certificates,
            checksum,
            command_line,
            compare,
            connections,
            diagnostics,
            drives,
            file,
            file_ops,
            hostkeys,
            import,
            login_scripts,
            multi_rename,
            mcp,
            manager,
            ops,
            panel,
            remote,
            properties,
            plugins,
            search,
            secrets,
            sessions,
            settings,
            ssh_exec,
            ssh_terminal,
            terminal,
            vault,
            viewer,
            wsl,
            tabs,
            history,
            hotlist,
            queue,
            app,
            update,
        ] }
    };
}

/// Writes `GROUPS` from [`crate::command_groups!`].
macro_rules! groups_table {
    ([$($group:ident,)*]) => {
        /// The groups' tables, in the order of [`crate::command_groups!`], each sorted by id.
        const GROUPS: &[&[CommandSpec]] = &[$($group::COMMANDS,)*];
    };
}

crate::command_groups!(groups_table);

/// How many commands there are.
const LEN: usize = {
    let mut len = 0;
    let mut group = 0;
    while group < GROUPS.len() {
        len += GROUPS[group].len();
        group += 1;
    }
    len
};

/// The groups' rows, one after the other.
const ALL: [CommandSpec; LEN] = {
    let mut all = [CommandSpec {
        id: "",
        class: CommandClass::Read,
        summary: "",
    }; LEN];
    let mut at = 0;
    let mut group = 0;
    while group < GROUPS.len() {
        let mut row = 0;
        while row < GROUPS[group].len() {
            all[at] = GROUPS[group][row];
            at += 1;
            row += 1;
        }
        group += 1;
    }
    all
};

/// Every command: the groups in the order of `GROUPS`, each sorted by id.
pub const COMMANDS: &[CommandSpec] = &ALL;

/// The rows of [`COMMANDS`] by id, built on the first lookup: the groups are sorted one by one,
/// not as a whole (`registry_tests`), so a dispatch looks its row up here instead of walking
/// the table.
static BY_ID: LazyLock<HashMap<&'static str, &'static CommandSpec>> =
    LazyLock::new(|| COMMANDS.iter().map(|spec| (spec.id, spec)).collect());

/// The registry row of `id`.
#[must_use]
pub fn spec(id: &str) -> Option<&'static CommandSpec> {
    BY_ID.get(id).copied()
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "registry_samples.rs"]
mod samples;

#[cfg(test)]
#[path = "registry_samples_more.rs"]
mod samples_more;
