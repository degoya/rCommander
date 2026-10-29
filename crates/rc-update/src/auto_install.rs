//! Installing an offered update by itself (RC-100-07): when, not how.
//!
//! Off by default. `on_quit` installs a downloaded offer when the window ends (`app.quit`, the
//! window's X, Alt+F4) and leaves the new version to the next start a person makes; `when_idle`
//! also restarts into it once nothing has happened for `update.idle_minutes`: no input, no job in
//! the queue, no terminal or SSH session, no dialog. Every condition that breaks starts the quiet
//! period again. Never by itself: an installation that does not install itself, a pre-release on
//! stable (the offer never carries one), or a version whose install failed or was taken back.
//! The caller observes and asks; this decides.
//!
//! Adapted from rDownloader's `crates/rd-update/src/auto_install.rs` and `auto_restart.rs` at
//! commit 26c8592a6: no time window - at a desk, quitting and idleness take its place.

use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};

/// The setting `update.auto_install`.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoMode {
    #[default]
    Off,
    OnQuit,
    WhenIdle,
}

impl AutoMode {
    /// The setting's value; anything unknown reads as off, the safe default.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value {
            "on_quit" => Self::OnQuit,
            "when_idle" => Self::WhenIdle,
            _ => Self::Off,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::OnQuit => "on_quit",
            Self::WhenIdle => "when_idle",
        }
    }
}

/// What the window sees at one moment.
#[derive(Clone, Copy, Debug, Default)]
pub struct Moment<'a> {
    pub mode: AutoMode,
    /// The installation updates itself (`InstallKind::Portable`).
    pub installs_itself: bool,
    /// The version offered and downloaded, if any.
    pub ready: Option<&'a str>,
    /// The install of that version failed or was taken back before.
    pub failed_before: bool,
    /// When the last key or click arrived.
    pub last_input: Option<DateTime<Utc>>,
    /// A job runs, waits or is paused in the queue.
    pub jobs: bool,
    /// A terminal or an SSH session is open.
    pub sessions: bool,
    /// A dialog is open.
    pub dialog: bool,
}

/// Why nothing happens now.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Wait {
    Off,
    Unsupported,
    NothingReady,
    FailedBefore,
    /// Only at quitting (`on_quit`), or quitting asked about the queue first.
    UntilQuit,
    Input,
    Jobs,
    Sessions,
    Dialog,
    /// Nothing breaks the quiet, but not yet for long enough.
    Settling,
}

impl Wait {
    /// The name `update.status` answers.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Unsupported => "unsupported",
            Self::NothingReady => "nothing_ready",
            Self::FailedBefore => "failed_before",
            Self::UntilQuit => "until_quit",
            Self::Input => "input",
            Self::Jobs => "jobs",
            Self::Sessions => "sessions",
            Self::Dialog => "dialog",
            Self::Settling => "settling",
        }
    }
}

/// Whether quitting installs the ready offer: on in `on_quit` and `when_idle`, an installation
/// that updates itself, a version that never failed, and no job in the queue (quitting with jobs
/// asks as it always did and ends without an update).
///
/// # Errors
///
/// Why not.
pub fn on_quit(moment: &Moment<'_>) -> Result<(), Wait> {
    common(moment)?;
    if moment.jobs {
        return Err(Wait::Jobs);
    }
    Ok(())
}

fn common(moment: &Moment<'_>) -> Result<(), Wait> {
    if moment.mode == AutoMode::Off {
        return Err(Wait::Off);
    }
    if !moment.installs_itself {
        return Err(Wait::Unsupported);
    }
    if moment.ready.is_none() {
        return Err(Wait::NothingReady);
    }
    if moment.failed_before {
        return Err(Wait::FailedBefore);
    }
    Ok(())
}

/// Follows how long nothing has happened, and decides whether `when_idle` restarts now.
#[derive(Clone, Debug, Default)]
pub struct IdleClock {
    /// Since when nothing broke the quiet; `None` while something does.
    quiet_since: Option<DateTime<Utc>>,
}

impl IdleClock {
    /// Observes `moment` at `now`: `Ok` when the window should install and restart now.
    ///
    /// # Errors
    ///
    /// Why not; every condition but the quiet itself starts the quiet period again.
    pub fn observe(
        &mut self,
        moment: &Moment<'_>,
        idle: TimeDelta,
        now: DateTime<Utc>,
    ) -> Result<(), Wait> {
        if let Err(wait) = common(moment) {
            self.quiet_since = None;
            return Err(wait);
        }
        if moment.mode != AutoMode::WhenIdle {
            self.quiet_since = None;
            return Err(Wait::UntilQuit);
        }
        let breaks = if moment.jobs {
            Some(Wait::Jobs)
        } else if moment.sessions {
            Some(Wait::Sessions)
        } else if moment.dialog {
            Some(Wait::Dialog)
        } else if moment.last_input.is_some_and(|input| now - input < idle) {
            Some(Wait::Input)
        } else {
            None
        };
        if let Some(wait) = breaks {
            self.quiet_since = None;
            return Err(wait);
        }
        let since = *self.quiet_since.get_or_insert(now);
        // The quiet counts from the later of the last input and the first quiet observation.
        let since = moment.last_input.map_or(since, |input| since.max(input));
        if now - since >= idle {
            Ok(())
        } else {
            Err(Wait::Settling)
        }
    }
}

#[cfg(test)]
#[path = "auto_install_tests.rs"]
mod tests;
