//! Reading the panels again when the person comes back from another program (owner,
//! 2026-11-01; RC-100-69): a file saved in an editor meanwhile shows up without Ctrl+R.
//!
//! The system tells each window's focus (`subscription.rs`, `QueueMsg::Focused`). A move between
//! rCommander's own windows - the main window and a dialog's - is an `Unfocused` followed at once
//! by a `Focused`; the app came back from another program only when no own window had the
//! keyboard for at least [`AWAY`] ([`Refocus::seen`], the clock passed in). Then each shown panel
//! that lists a plain folder on the local backend (WSL shares included) is read again with
//! `panel.refresh`, which keeps cursor, selection and scroll position ([`refreshes`]); a panel on
//! a network drive or share only with `panels.refresh_on_focus_network`, a server or an archive
//! never. A panel showing its terminal, a flat view (its reading would start over) and a listing
//! still arriving are left alone. `panels.refresh_on_focus` switches it all off. No MCP: this is
//! the window's behaviour, an agent has `panel.refresh`.
//!
//! The same focus events keep a modal dialog modal on Linux and macOS (RC-080-73): a focus that
//! lands on a window under the open question or modal dialog goes back to it
//! (`dialog_windows/front.rs`). Windows keeps the focus off a locked window itself.

use std::time::{Duration, Instant};

use iced::{Task, window};
use rc_engine::{
    Command, Drive, DriveKind, PanelId, PanelState, PanelView, VfsPath, drive_of,
    settings::{REFRESH_ON_FOCUS_KEY, REFRESH_ON_FOCUS_NETWORK_KEY, stored_bool},
};
use rc_vfs::{BackendId, network_path};

use super::{App, Effect, Message};

/// How long no own window may have had the keyboard before a `Focused` counts as coming back
/// from another program; a switch between own windows is far quicker.
pub const AWAY: Duration = Duration::from_millis(300);

/// Which of the app's windows has the keyboard, and since when none has.
#[derive(Debug, Default)]
pub struct Refocus {
    /// The window that has the keyboard, as the system last said.
    focused: Option<window::Id>,
    /// When the last own window let go of the keyboard with no other taking it.
    left: Option<Instant>,
}

impl Refocus {
    /// Window `id` took the keyboard (`focused`) or let go of it at `now`; `true` when this is
    /// the app coming back from another program. The first focus after the start is not. When
    /// the system tells the newly focused window first, the old one's `Unfocused` that follows
    /// changes nothing.
    pub fn seen(&mut self, id: window::Id, focused: bool, now: Instant) -> bool {
        if focused {
            let away = self.focused.is_none();
            self.focused = Some(id);
            let left = self.left.take();
            away && left.is_some_and(|left| now.saturating_duration_since(left) >= AWAY)
        } else {
            if self.focused == Some(id) {
                self.focused = None;
                self.left = Some(now);
            }
            false
        }
    }
}

/// The two settings, as read when the app came back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Switches {
    /// `panels.refresh_on_focus`.
    pub on: bool,
    /// `panels.refresh_on_focus_network`.
    pub network: bool,
}

impl Default for Switches {
    /// The schema's defaults: on, network drives off.
    fn default() -> Self {
        Self {
            on: true,
            network: false,
        }
    }
}

/// What the choice looks at in a panel.
#[derive(Debug, Clone, Copy)]
pub struct Shown<'a> {
    pub path: &'a VfsPath,
    /// The file list, not the terminal.
    pub files: bool,
    /// A plain folder, no flat view (branch view, search hits).
    pub plain: bool,
    /// No listing still arriving: a refresh would read the folder a second time.
    pub settled: bool,
}

impl<'a> From<&'a PanelState> for Shown<'a> {
    fn from(state: &'a PanelState) -> Self {
        Self {
            path: &state.path,
            files: state.view == PanelView::Files,
            plain: state.flat.is_none(),
            settled: state.listing.is_none(),
        }
    }
}

/// Whether a panel showing `shown` is read again on the app's return; `network` when its folder
/// lies on a network drive or share ([`on_network`]).
#[must_use]
pub fn refreshes(shown: Shown<'_>, network: bool, switches: Switches) -> bool {
    switches.on
        && shown.files
        && shown.plain
        && shown.settled
        && shown.path.backend == BackendId::Local
        && (switches.network || !network)
}

/// Whether `path` lies on another machine: a share by its form (`\\server\share`; a WSL
/// distribution's share is this machine's) or a network drive of the drive bar's list. Judged
/// without touching the file system.
#[must_use]
pub fn on_network(path: &VfsPath, drives: &[Drive]) -> bool {
    path.backend == BackendId::Local
        && (network_path(&path.path.to_string_lossy()).is_some()
            || drive_of(drives, path).is_some_and(|drive| drive.kind == DriveKind::Network))
}

impl App {
    /// Window `id` took the keyboard or let go of it: a focus under a question or a modal
    /// dialog goes back to it ([`App::modal_focus`]); on the app's return from another program
    /// the switches are read, then the panels follow ([`App::refresh_on_return`]).
    pub(in crate::app) fn refocused(&mut self, id: window::Id, focused: bool) -> Vec<Effect> {
        let now = Instant::now();
        let mut effects = self.modal_focus(id, focused, now);
        if !self.dialog_windows.refocus.seen(id, focused, now) {
            return effects;
        }
        let Some(db) = self.db.clone() else {
            effects.extend(self.refresh_on_return(Switches::default()));
            return effects;
        };
        let read = async move {
            Switches {
                on: stored_bool(&db, REFRESH_ON_FOCUS_KEY).await,
                network: stored_bool(&db, REFRESH_ON_FOCUS_NETWORK_KEY).await,
            }
        };
        effects.push(Effect::Task(Task::perform(read, Message::Refocus)));
        effects
    }

    /// Linux and macOS: window `id` took the keyboard at `now` while it lies under the open
    /// question or modal dialog - the window on top takes it back (`front.rs`). Not on Windows,
    /// where such a window is disabled and never takes it.
    fn modal_focus(&mut self, id: window::Id, focused: bool, now: Instant) -> Vec<Effect> {
        if cfg!(windows) || !focused {
            return Vec::new();
        }
        self.dialog_windows
            .hand_back(id, now)
            .map(|top| Effect::Task(window::gain_focus(top)))
            .into_iter()
            .collect()
    }

    /// `panel.refresh` for each shown panel [`refreshes`] picks.
    pub(in crate::app) fn refresh_on_return(&self, switches: Switches) -> Vec<Effect> {
        PanelId::BOTH
            .into_iter()
            .filter(|&panel| {
                let state = &self.pane(panel).state;
                let network = on_network(&state.path, &self.drive_bar.drives);
                refreshes(Shown::from(state), network, switches)
            })
            .map(|panel| Effect::Dispatch(panel, vec![Command::Refresh { panel: Some(panel) }]))
            .collect()
    }
}

#[cfg(test)]
#[path = "refocus_tests.rs"]
mod tests;
