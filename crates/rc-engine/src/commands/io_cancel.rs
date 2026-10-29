//! A drive or share that does not answer (RC-080-04): every call on this machine's drives ends
//! within its time limit (`rc_vfs::TimeoutFs`), and Escape ends it sooner. The window's commands
//! run one after the other, so `panel.cancel_io` is the one the window sends past them: it only
//! flips a switch, and the waiting command ends with `cancelled`.

use std::time::Duration;

use crate::{Engine, Outcome, PanelId};

/// How long a call waits before the window lets Escape stop it: a quick listing is never caught.
pub(crate) const IO_STALL: Duration = Duration::from_millis(500);

impl Engine {
    /// `panel.cancel_io`: the calls on this machine's drives that wait now end with `cancelled`;
    /// the panel stays where it was. The queue's copies are not touched.
    pub(crate) fn cancel_io(&self, panel: Option<PanelId>) -> Outcome {
        let stopped = self.inner.io_cancel.cancel();
        tracing::debug!(stopped, "waiting calls on local drives cancelled");
        Outcome::Panel(self.snapshot(panel))
    }

    /// Whether a call on this machine's drives has waited half a second or longer: Escape then
    /// stops it (`panel.cancel_io`). A read-out for the window, no command.
    #[must_use]
    pub fn io_stalled(&self) -> bool {
        self.inner.io_cancel.stalled(IO_STALL)
    }
}

#[cfg(test)]
#[path = "io_cancel_tests.rs"]
mod tests;
