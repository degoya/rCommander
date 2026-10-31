//! What `update.status` answers and the window shows (RC-100-05 ... RC-100-07), and why an
//! update command was refused. Paths, versions, codes and sizes: no secret, no installation id.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The self-update as `update.status` answers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateStatus {
    pub running_version: String,
    /// `stable` or `beta`, as in force: chosen, or the running version's.
    pub channel: String,
    /// Whether a person chose the channel; otherwise it follows the running version.
    pub channel_chosen: bool,
    pub check_enabled: bool,
    pub interval_hours: u32,
    /// `official` (the GitHub releases) or the mirror's address.
    pub source: String,
    /// When the last check ended, and its refusal's code (`None`: it went through).
    pub last_check: Option<DateTime<Utc>>,
    pub last_code: Option<String>,
    /// The verified offer of the last check, with its notes.
    pub offer: Option<rc_update::Offer>,
    pub download: DownloadState,
    /// `portable`, a package manager's kind (`scoop`, `homebrew`, `winget`, `deb`, `rpm`) or
    /// `unknown`, and for all but `portable` the reason's code (RC-100-52).
    pub install_kind: String,
    pub install_reason: Option<String>,
    /// The manager's command that updates this installation (`scoop update rcommander`, ...);
    /// `None` for `portable` and `unknown`. A manager's installation never replaces itself.
    pub install_command: Option<String>,
    /// Handed to the updater; the window ends now.
    pub installing: bool,
    /// A job runs, waits or is paused: the install waits for the empty queue.
    pub queue_busy: bool,
    /// The last update the journal records.
    pub last_update: Option<LastUpdate>,
    /// "Updated to X": set while X runs and its notice was not shown yet.
    pub updated_to: Option<String>,
    /// `off`, `on_quit` or `when_idle`, and what `when_idle` waits for now.
    pub auto_install: String,
    pub auto_waiting_for: Option<String>,
    /// Bytes the database copies, downloads and the trash of updates take.
    pub leftover_bytes: u64,
}

/// Where the download of the offer stands.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DownloadState {
    #[default]
    None,
    Running {
        received: u64,
        total: u64,
    },
    /// The verified file, ready to install.
    Ready {
        file: String,
    },
    Failed {
        code: String,
    },
}

/// The last update the journal records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastUpdate {
    pub phase: String,
    pub from_version: String,
    pub target_version: String,
    /// Why it failed or was rolled back.
    pub reason: Option<String>,
}

/// Why an update command was refused; [`UpdateRefusal::code`] is the stable code.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{detail}")]
pub struct UpdateRefusal {
    code: &'static str,
    detail: String,
}

/// The codes of the engine's own refusals; a check, a download or an install carries one of
/// `rc_update::UPDATE_ERROR_CODES` or `rc_update::install::INSTALL_CODES` instead.
pub const REFUSAL_CODES: &[&str] = &[
    "update.not_started",
    "update.no_offer",
    "update.no_artifact",
    "update.not_downloaded",
    "update.download_running",
    "update.install_unsupported",
    "update.queue_busy",
    "update.handed",
];

impl UpdateRefusal {
    pub(crate) fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Every code a refusal carries.
    #[must_use]
    pub fn all_codes() -> Vec<&'static str> {
        // The install reports a failed download or a wrong digest with the download's codes.
        let mut codes: Vec<&'static str> = Vec::new();
        for code in REFUSAL_CODES
            .iter()
            .chain(rc_update::UPDATE_ERROR_CODES)
            .chain(rc_update::install::INSTALL_CODES)
        {
            if !codes.contains(code) {
                codes.push(code);
            }
        }
        codes
    }
}

impl From<rc_update::UpdateError> for UpdateRefusal {
    fn from(error: rc_update::UpdateError) -> Self {
        Self::new(error.code(), error.to_string())
    }
}

impl From<rc_update::install::InstallError> for UpdateRefusal {
    fn from(error: rc_update::install::InstallError) -> Self {
        Self::new(error.code, error.detail)
    }
}
