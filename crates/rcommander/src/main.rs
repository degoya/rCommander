//! The `rcommander` binary: the window without a subcommand - two panels over `rc-engine`, with
//! the MCP server on 127.0.0.1 inside it - the MCP server over stdio with `mcp`, a backup of the
//! database with `backup <target>`, and the plugin author's tools with `plugin ...`.

// A GUI program on Windows: no console window opens behind it. The price is that `--version`
// and `--help` print nothing when started from a console (it has none attached); `backup`
// and `plugin` attach to their caller's console (`console.rs`).
#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod backup_cli;
mod console;
mod format;
mod gpu_loss;
mod i18n;
mod keymap;
mod locale;
mod mcp;
#[cfg(windows)]
mod native_window;
mod platform;
mod plugin_cli;
mod term;
mod theme;
mod update_cli;

use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};

/// The version line of `--version`: the package version, the commit and the build time that
/// `build.rs` compiled in.
const LONG_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("RC_BUILD_COMMIT"),
    ", built ",
    env!("RC_BUILD_TIME"),
    ")"
);

#[derive(Parser)]
#[command(name = "rcommander", version, long_version = LONG_VERSION, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    // The hidden `--screenshots` run (RC-100-45).
    #[command(flatten)]
    shots: app::screenshots::Flags,
}

#[derive(Subcommand)]
enum Command {
    /// Serve rCommander's tools over MCP on stdin/stdout.
    Mcp,
    /// Back up the database to a file, or to a folder under a generated name; works while the
    /// window runs. Exits with 0 only for a backup that passed its integrity check.
    Backup {
        /// The backup file, or a folder for rcommander-YYYYMMDD-HHMMSS.db.
        target: PathBuf,
        /// Replace the file if it exists.
        #[arg(long)]
        overwrite: bool,
    },
    /// Write, package, sign, verify and check plugins (new, package, verify, conformance,
    /// keygen).
    Plugin {
        #[command(subcommand)]
        command: plugin_cli::PluginCommand,
    },
    /// The updater the window starts from a copy of itself (RC-100-06); hidden.
    #[command(hide = true)]
    ApplyUpdate {
        #[arg(long)]
        journal: PathBuf,
    },
}

fn main() -> ExitCode {
    // The About page and `app.about` name the build, in the window and over `rcommander mcp`.
    rc_engine::about::stamp_build(env!("RC_BUILD_COMMIT"), env!("RC_BUILD_TIME"));
    let cli = Cli::parse();
    if let Some(code) = app::screenshots::run_if_asked(&cli.shots) {
        return code;
    }
    match cli.command {
        // An update the last run left half done ends before the window and its database open.
        None if update_cli::recover_before_window() => ExitCode::SUCCESS,
        None => match app::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("rcommander: {error}");
                ExitCode::FAILURE
            }
        },
        Some(Command::Backup { target, overwrite }) => backup_cli::run(&target, overwrite),
        Some(Command::Plugin { command }) => plugin_cli::run(command),
        Some(Command::ApplyUpdate { journal }) => update_cli::apply(&journal),
        Some(Command::Mcp) => {
            let served = match tokio::runtime::Runtime::new() {
                Ok(runtime) => runtime
                    .block_on(rc_mcp::run_stdio())
                    .map_err(|error| format!("{error:#}")),
                Err(error) => Err(error.to_string()),
            };
            // A headless engine's copies of files opened from archives.
            rc_engine::unpacked::remove_own();
            match served {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("rcommander mcp: {error}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
