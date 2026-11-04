//! A drag of local entries that the system runs from its start, as in Total Commander and the
//! Explorer (RC-080-01): once the pointer passes the threshold, `drag_out` hands the files to the
//! shell's drag loop, which draws the drag image and the copy and move cursors, follows the
//! pointer over every window and keeps going through Alt+Tab. A drop back onto rCommander's own
//! window comes in as dropped files; when they are the dragged ones, the drop opens the copy
//! dialog of F5 (the move dialog with Shift) towards the other panel, as a drag inside the window
//! always did, and a drop on the panel it came from does nothing. Remote entries keep the drag
//! inside the window and are fetched only when they leave it (`fetch.rs`): a drag between the
//! panels must not copy a server's files to a temporary folder first.

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use rc_engine::PanelId;

use super::{super::App, Effect, dropped, fetch, start_drag};
use crate::platform::drag_out::{self, DragOutcome, Pointer};

/// How long after the system's drag loop ended its own dropped files may still arrive: the
/// window hands a drop over one file at a time, after the loop returned.
const LATE: Duration = Duration::from_secs(2);

/// The drag the system runs for entries of a panel.
#[derive(Debug, Clone)]
pub(super) struct OwnDrag {
    pub(super) from: PanelId,
    pub(super) names: Vec<String>,
    /// The dragged files as the drop hands them back.
    pub(super) paths: Vec<PathBuf>,
    /// When the system's drag loop returned.
    pub(super) ended: Option<Instant>,
}

impl OwnDrag {
    /// Whether the files `dropped` at `now` are this drag's own: all of them dragged, and the
    /// loop still running or just over.
    pub(super) fn takes(&self, dropped: &[PathBuf], now: Instant) -> bool {
        !dropped.is_empty()
            && dropped.iter().all(|path| self.paths.contains(path))
            && self
                .ended
                .is_none_or(|ended| now.saturating_duration_since(ended) <= LATE)
    }
}

impl App {
    /// The drag of `names` from `panel` just passed the threshold: for a local folder the system
    /// takes it over at once; `None` where it does not (a remote folder, no system drag), and the
    /// drag goes on inside the window.
    pub(super) fn own_drag(&mut self, panel: PanelId, names: &[String]) -> Option<Vec<Effect>> {
        let folder = self.pane(panel).state.path.clone();
        if !drag_out::AVAILABLE || names.is_empty() || fetch::remote(&folder) {
            return None;
        }
        let paths: Vec<PathBuf> = names.iter().map(|name| folder.path.join(name)).collect();
        self.drag.own = Some(OwnDrag {
            from: panel,
            names: names.to_vec(),
            paths: paths.clone(),
            ended: None,
        });
        Some(start_drag(panel, paths, None))
    }

    /// The system's drag loop returned. Its drop on the window may still be on its way; a drag
    /// that never started (the button was up already) is dropped where the pointer stands, as a
    /// release inside the window is.
    pub(super) fn own_ended(&mut self, outcome: DragOutcome) -> Option<Vec<Effect>> {
        let own = self.drag.own.as_mut()?;
        match outcome {
            DragOutcome::NotHeld => {
                let own = self.drag.own.take()?;
                let shift = self.modifiers_held().shift;
                Some(self.drop_on_panel(own.from, self.hovered, own.names, shift))
            }
            DragOutcome::Ended | DragOutcome::Failed => {
                own.ended = Some(Instant::now());
                None
            }
        }
    }

    /// A drop on the window settled: when it is the system's drag of our own entries, the copy
    /// (or move) dialog towards the panel under the pointer; `None` for another program's files.
    pub(super) fn own_dropped(
        &mut self,
        paths: &[PathBuf],
        pointer: Option<Pointer>,
    ) -> Option<Vec<Effect>> {
        if !self
            .drag
            .own
            .as_ref()
            .is_some_and(|own| own.takes(paths, Instant::now()))
        {
            return None;
        }
        let own = self.drag.own.take()?;
        let across = pointer.map(|pointer| pointer.across);
        let share = self.resize.left_share();
        let to = dropped::target_panel(across, share, self.maximized()).or(self.hovered);
        let shift = pointer.map_or(self.modifiers_held().shift, |pointer| pointer.shift);
        Some(self.drop_on_panel(own.from, to, own.names, shift))
    }
}

#[cfg(test)]
#[path = "own_tests.rs"]
mod tests;
