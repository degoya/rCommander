//! The self-update's process half (RC-100-06): the hidden `rcommander apply-update --journal
//! <path>` the window starts from a copy of itself in `<data>/update/updater/`, and what a start
//! does with an update before it opens the database.
//!
//! `apply-update` is `rc_update::install::apply::run`: it waits for the window to end, switches
//! the files, starts the new version and waits for its proof in the journal, or takes the switch
//! back. Its output goes to `<data>/update/updater.log` (the window started it so); it opens no
//! window and no console.

use std::path::Path;
use std::process::ExitCode;

use rc_update::install::recover::{Recovery, recover_at_start};

/// `rcommander apply-update --journal <path>`.
pub fn apply(journal: &Path) -> ExitCode {
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init();
    let free_space = |folder: &Path| rc_vfs::disk_space(folder).ok().map(|(free, _)| free);
    match rc_update::install::apply::run(journal, &free_space) {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(error) => {
            eprintln!("rcommander apply-update: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Before the window opens and its database: ends or continues an update the journal records.
/// `true` when this process started the restored previous program in its place and must end.
pub fn recover_before_window() -> bool {
    let Some(data) = rc_db::Db::default_path()
        .ok()
        .and_then(|database| database.parent().map(Path::to_path_buf))
    else {
        return false;
    };
    let Ok(executable) = std::env::current_exe() else {
        return false;
    };
    match recover_at_start(&data, &executable, env!("CARGO_PKG_VERSION")) {
        Ok(Recovery::Continue) => false,
        Ok(Recovery::Restart(program)) => {
            // console-window: allow (rcommander is a GUI program and opens no console of its own)
            let started = std::process::Command::new(&program)
                .args(std::env::args_os().skip(1))
                .spawn();
            match started {
                Ok(_) => true,
                Err(error) => {
                    eprintln!("rcommander: the previous version did not start: {error}");
                    false
                }
            }
        }
        Err(error) => {
            // The program folder may hold two versions: the manual recovery of
            // docs/development.md, "Taking an update back by hand".
            eprintln!("rcommander: the last update could not be taken back: {error:#}");
            false
        }
    }
}
