//! The hint of a panel on a portable device that takes a transfer (RC-110-16).
//!
//! A device answers one call at a time: while a job writes onto it, its folder cannot be read
//! and the panel shows it as it was before. With `panels.device_transfer_hint` on (the default)
//! the panel's status line says so - the file at work and how far the job is - from the jobs the
//! window follows ([`transfer_on`]), on every change of a job. Ctrl+R meanwhile is the engine's
//! refusal `device_busy` (`rc_engine::device_transfer`), shown in the same line; it goes with
//! the hint once the job is over, when the panels are read again (`run.rs`). A job that ends
//! while another still writes onto the device leaves that device's panel unread: its reading
//! would be refused and keep the other panel's from running ([`App::waits_for_device`]).

use rc_engine::{BackendId, Command, JobSnapshot, PanelId, device_transfer::writing_to};

use super::jobs::sources_of;
use crate::{
    app::App,
    i18n::{error_text, t},
};

/// The engine's code for a reading refused while the device takes a transfer.
const BUSY: &str = "device_busy";

/// The transfer onto the device `backend` among `jobs`: the name of the file at work - before
/// the first file, of the job's first source - and the share of the job done in percent.
pub(super) fn transfer_on<'a>(
    backend: &BackendId,
    jobs: impl IntoIterator<Item = &'a JobSnapshot>,
) -> Option<(String, u8)> {
    let job = writing_to(backend, jobs)?;
    let file = job
        .progress
        .current
        .as_ref()
        .or_else(|| sources_of(&job.kind).into_iter().next());
    let name = file
        .and_then(|file| file.path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    // By bytes, or by files where there are none; rounded down, so 100 only at the end.
    let progress = &job.progress;
    let (done, total) = if progress.bytes_total > 0 {
        (progress.bytes_done, progress.bytes_total)
    } else {
        (progress.files_done, progress.files_total)
    };
    let percent = (u128::from(done.min(total)) * 100)
        .checked_div(u128::from(total))
        .and_then(|percent| u8::try_from(percent).ok())
        .unwrap_or(0);
    Some((name, percent))
}

impl App {
    /// Sets or takes away each panel's hint, as the jobs stand now. A reading refused meanwhile
    /// (`device_busy`) leaves the status line with the hint.
    pub(super) fn show_device_hints(&mut self) {
        for panel in PanelId::BOTH {
            let hint = if self.file_ops.hide_device_hint {
                None
            } else {
                let backend = &self.pane(panel).state.path.backend;
                transfer_on(backend, self.file_ops.jobs.snapshots()).map(|(name, percent)| {
                    t!("panel.device_transfer", name = name, percent = percent)
                })
            };
            let pane = self.pane_mut(panel);
            if hint.is_none()
                && pane.transfer_hint.is_some()
                && pane.error.as_deref() == Some(error_text(BUSY).as_str())
            {
                pane.error = None;
            }
            pane.transfer_hint = hint;
        }
    }

    /// Whether `command` reads a panel again whose device a followed job still writes to.
    pub(super) fn waits_for_device(&self, command: &Command) -> bool {
        match command {
            Command::Refresh { panel: Some(panel) } => self.pane(*panel).transfer_hint.is_some(),
            _ => false,
        }
    }
}

#[cfg(test)]
#[path = "device_hint_tests.rs"]
mod tests;
