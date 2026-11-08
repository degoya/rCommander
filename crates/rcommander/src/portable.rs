//! The portable mode at the start (RC-100-95): with the marker `rcommander.portable` beside the
//! program, `data/` beside it is the data folder of this process - the database, the plugins,
//! the backups, the MCP instance file, the update's journal and copies. Decided once, first in
//! `main`, before the update's recovery and before anything opens the database
//! (`rc_db::set_location`).
//!
//! The marker counts only where `rc_update::portable_mode::check` lets it: never for a package
//! manager's installation, a folder this process cannot write, a network drive, a linked `data`
//! or on macOS. Then the start goes on with the person's own data folder and the status line
//! says why ([`refusal`]) - unlike Total Commander, which runs on with a read-only ini and offers
//! to copy it into the profile only when saving fails: rCommander writes to SQLite all the time,
//! and a read-only database does not carry a session.
//!
//! There is no setting for it: the settings live in the data folder this decides. The way back
//! is to delete the marker (the wiki's installation page).

use std::{path::Path, sync::OnceLock};

use rc_db::DataLocation;
use rc_update::{
    install_kind::{self, Installation},
    portable_mode,
};

/// Why the marker was passed over, once decided.
static REFUSAL: OnceLock<&'static str> = OnceLock::new();

/// What the start decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// The portable folder; `None` for the person's own.
    pub portable: Option<DataLocation>,
    /// Why a marker that is there does not count, a code with an `error.<code>` text.
    pub refusal: Option<&'static str>,
}

/// Decides where this process keeps its data and hands it to `rc_db`. The first call counts.
pub fn choose() {
    let Ok(executable) = std::env::current_exe() else {
        return;
    };
    let decision = decide(&executable, install_kind::detect);
    if let Some(code) = decision.refusal {
        let _ = REFUSAL.set(code);
    }
    if let Some(location) = decision.portable {
        let _ = rc_db::set_location(location);
    }
}

/// The decision for the program at `executable`, its installation found by `detect` (only when
/// the marker is there).
pub fn decide(executable: &Path, detect: impl FnOnce(&Path) -> Installation) -> Decision {
    let folder = executable.parent().unwrap_or(executable);
    if !portable_mode::marker_present(folder) {
        return Decision {
            portable: None,
            refusal: None,
        };
    }
    match portable_mode::check(&detect(executable)) {
        Ok(data) => Decision {
            portable: Some(DataLocation::Portable(data)),
            refusal: None,
        },
        Err(code) => Decision {
            portable: None,
            refusal: Some(code),
        },
    }
}

/// Why this start passed the marker over, for the status line.
pub fn refusal() -> Option<&'static str> {
    REFUSAL.get().copied()
}

/// Whether the database dialog offers to make this copy portable: not on macOS, whose signed
/// bundle a marker would break, and not when it is portable already.
pub fn offered() -> bool {
    !cfg!(target_os = "macos") && !rc_db::is_portable()
}

#[cfg(test)]
#[path = "portable_tests.rs"]
mod tests;
