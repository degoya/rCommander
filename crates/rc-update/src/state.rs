//! What an installation remembers about its updates between starts (RC-100-05, RC-100-07): the
//! replay floors, the last check and its result, the verified offer, and the versions whose
//! install failed. The engine keeps it as the one setting `update.state` (no schema row, like
//! `mcp.token`; no table, nothing that grows with use) and leaves it out of the export, since it
//! belongs to this installation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{check::Floors, offer::Offer};

/// The setting the state is stored under.
pub const STATE_KEY: &str = "update.state";

/// Most failed versions remembered; older ones fall out (a newer version clears them anyway).
const MAX_FAILED: usize = 8;

/// The stored state; every field defaults, so a state from an older build still reads.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct UpdateState {
    /// The highest sequence accepted per channel; stored before anything acts on a check.
    #[serde(default)]
    pub floors: Floors,
    /// When the last check ended.
    #[serde(default)]
    pub last_check: Option<DateTime<Utc>>,
    /// Its refusal's code, `None` when it went through.
    #[serde(default)]
    pub last_code: Option<String>,
    /// The verified offer of the last check that went through.
    #[serde(default)]
    pub offer: Option<Offer>,
    /// Versions whose install failed or was rolled back: never installed again by itself
    /// (RC-100-07) until a person asks or a newer version comes.
    #[serde(default)]
    pub failed: Vec<String>,
    /// The version whose "Updated to X" was shown, so it shows once (RC-100-06).
    #[serde(default)]
    pub announced: Option<String>,
}

impl UpdateState {
    /// Records a failed or rolled-back install of `version`.
    pub fn record_failure(&mut self, version: &str) {
        if !self.failed.iter().any(|failed| failed == version) {
            self.failed.push(version.to_owned());
        }
        while self.failed.len() > MAX_FAILED {
            self.failed.remove(0);
        }
    }

    /// Whether an install of `version` failed before.
    #[must_use]
    pub fn failed_before(&self, version: &str) -> bool {
        self.failed.iter().any(|failed| failed == version)
    }

    /// Reads a stored value; anything unreadable is a fresh state (a warning, never a failure:
    /// the floors start again from the manifests' expiry, which bounds a replay).
    #[must_use]
    pub fn from_value(value: Option<serde_json::Value>) -> Self {
        match value.map(serde_json::from_value::<Self>) {
            Some(Ok(state)) => state,
            Some(Err(error)) => {
                tracing::warn!(%error, "the stored update state cannot be read; it starts afresh");
                Self::default()
            }
            None => Self::default(),
        }
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
