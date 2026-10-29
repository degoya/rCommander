//! rCommander's MCP server: every tool is a thin wrapper over an `rc-engine` command, so an
//! agent drives the same panels the keyboard does.
//!
//! Two transports: Streamable HTTP on `127.0.0.1` inside the running app ([`serve_http`],
//! bearer token, `Host`/`Origin` checks against DNS rebinding, an instance file for the stdio
//! side) and stdio as `rcommander mcp` ([`run_stdio`]: forwards to the running app, or starts
//! the engine headless).
//!
//! Every call passes one gate and one mask (`server.rs`): the tool's command class is held
//! against the per-class policy (`mcp.policy`: allowed, ask, deny), `ask` - always for
//! destructive and exec tools - waits for the person's answer in the window (`ask.rs`; headless,
//! a single-use confirmation code where the person allowed that, `confirm.rs`), and every answer
//! has its credentials masked (`mask.rs`). Before the gate, a path argument that reaches a share
//! on another machine is refused (`unc.rs`). `coverage.rs` says which tool covers
//! each engine command (the rows in `coverage/<group>.rs`, one file per registry group), and `tests/mcp_coverage.rs` fails when a command has no entry.
//! `config.rs` reads the switch and the port; switched off, neither transport serves.

#![warn(unreachable_pub)]

mod answer;
mod ask;
mod codes;
pub mod config;
mod confirm;
pub mod coverage;
mod freeze;
mod gate_settings;
mod guard;
mod guard_write;
mod http;
pub mod instance;
mod mask;
mod policy;
mod redact;
mod server;
mod stdio;
#[cfg(test)]
mod test_support;
mod token;
mod tools;
mod unc;

pub use codes::ALL_CODES;
pub use guard::sensitive_path;
pub use guard_write::protected_target;
pub use http::{HttpHandle, McpHttpConfig, serve_http};
pub use policy::{POLICY_KEY, Policy, Rule};
/// The folder of database backups beside `rcommander.db`, where `db_backup` writes; the MCP read
/// guard of RC-020-07 keeps agents out of it. One name for both, from `rc-db`.
pub use rc_db::BACKUP_DIR;
pub use server::{McpContext, RcMcpServer};
pub use stdio::{proxy_io, run_stdio, serve_io};
pub use token::{McpToken, TOKEN_KEY};
