//! `rcommander backup <target>`: a checked backup of `rcommander.db`, also while the window runs
//! (RC-010-12).
//!
//! The backup reads the file through a connection of its own - WAL lets it read beside the
//! window's writer - and runs no migration. A folder as the target gets a generated name
//! (`rcommander-YYYYMMDD-HHMMSS.db`), a file name is taken as it is; an existing file is replaced
//! only with `--overwrite`. The exit code is 0 only for a backup that passed its check.

use std::{path::Path, process::ExitCode};

use rc_db::{BackupReport, Db};

use crate::{console, i18n::t};

/// Runs the backup and prints its result to the caller's console.
pub fn run(target: &Path, overwrite: bool) -> ExitCode {
    console::attach_parent();
    match backup(target, overwrite) {
        Ok(report) => {
            println!(
                "{}",
                t!(
                    "cli.backup_done",
                    path = report.path.display(),
                    size = report.size
                )
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("rcommander backup: {error}");
            ExitCode::FAILURE
        }
    }
}

fn backup(target: &Path, overwrite: bool) -> Result<BackupReport, String> {
    let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
    runtime.block_on(async {
        let database = Db::default_path().map_err(|error| error.to_string())?;
        rc_db::backup_file(&database, target, overwrite)
            .await
            .map_err(|error| error.to_string())
    })
}
