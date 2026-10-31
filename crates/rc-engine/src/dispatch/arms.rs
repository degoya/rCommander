//! The arms of [`crate::Engine::dispatch`], one module per registry group: `arms/<group>.rs` for
//! `registry/<group>.rs`.
//!
//! Each module writes its group's arms in an `arms!` macro, as the registry writes its variants
//! in `commands!`: `dispatch.rs` hands the arms gathered so far to the next group's macro, in the
//! order of [`crate::command_groups!`], and with the last one writes a single `match` over all
//! of them. That `match` stays exhaustive - a [`crate::Command`] variant without an arm, or a
//! group missing from [`crate::command_groups!`], does not compile, and an arm in two groups is
//! an unreachable pattern.
//!
//! An arm is written as it would stand in the `match`, with `$self` for the engine. Its names
//! (`Command`, `Outcome`, `from_mcp`, ...) resolve in `dispatch.rs`, where the macros expand.

pub(super) mod agent;
pub(super) mod app;
pub(super) mod archive;
pub(super) mod archive_ops;
pub(super) mod backup;
pub(super) mod certificates;
pub(super) mod checksum;
pub(super) mod command_line;
pub(super) mod compare;
pub(super) mod connections;
pub(super) mod diagnostics;
pub(super) mod drives;
pub(super) mod file;
pub(super) mod file_ops;
pub(super) mod history;
pub(super) mod hostkeys;
pub(super) mod hotlist;
pub(super) mod import;
pub(super) mod login_scripts;
pub(super) mod manager;
pub(super) mod mcp;
pub(super) mod multi_rename;
pub(super) mod ops;
pub(super) mod panel;
pub(super) mod plugins;
pub(super) mod properties;
pub(super) mod queue;
pub(super) mod remote;
pub(super) mod search;
pub(super) mod secrets;
pub(super) mod sessions;
pub(super) mod settings;
pub(super) mod ssh_exec;
pub(super) mod ssh_terminal;
pub(super) mod tabs;
pub(super) mod terminal;
pub(super) mod theme;
pub(super) mod update;
pub(super) mod vault;
pub(super) mod viewer;
pub(super) mod wsl;
