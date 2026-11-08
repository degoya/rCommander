//! The panels' file watcher (RC-110-02): a shown folder is read again when something in it
//! changes, as Total Commander's "Auto-refresh when the file system changes" does.
//!
//! Watched are the folders of the two shown panels while they list a plain folder of this
//! machine (`choose.rs`), each alone, never its sub-folders: by the system's notices
//! (`native.rs`, the crate `notify`), a WSL share and a folder the system refuses by its time
//! once a second (`poll.rs`). A panel is read with [`Engine::refresh`], so its cursor and
//! selection stay; the readings are bundled (`coalesce.rs`), and a listing still arriving is
//! never cut short. The loop (`run.rs`) follows the panels through the engine's events and the
//! settings `panels.watch*` through [`EngineEvent::SettingChanged`].
//!
//! The app starts it ([`Engine::start_watching`]); the headless `rcommander mcp` does not. It
//! runs while `panels.watch` is off too: a shell back at its prompt reads the shown local folders
//! through it (`commands/terminal_watch.rs`). The window says when it is in the front
//! ([`Engine::set_foreground`]): with `panels.watch_background` off a change in the background
//! is only noted, and read once on the way back. Coming back from another program (RC-100-69)
//! skips a panel whose system watcher has reported nothing since it was last read
//! ([`Engine::watch_quiet`]). No MCP tool: an agent reads with `panel_refresh` and `list_dir`,
//! and `panel.toggle_watch` is `settings_set` of `panels.watch`.

mod choose;
mod coalesce;
mod native;
mod poll;
mod run;

use std::{
    path::PathBuf,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use serde_json::Value;
use tokio::sync::mpsc;

use crate::{Engine, EngineError, EngineEvent, Outcome, PanelChange, PanelId, settings::WATCH_KEY};

use native::Change;

/// How many notices wait for the loop; beyond, one full reading replaces them.
const CHANNEL: usize = 1024;

/// What reaches the watcher's loop besides the engine's events.
#[derive(Debug)]
pub(crate) enum Signal {
    /// The system reported a change of these paths.
    Changed { paths: Vec<PathBuf>, change: Change },
    /// The system lost notices: every watched panel is read.
    Rescan,
    /// A local shell is back at its prompt: the shown local file lists are read.
    Prompt,
    /// The window came to the front or left it.
    Foreground,
}

/// The watcher's side of the engine: the loop's channel and what the loop tells the window.
#[derive(Debug, Default)]
pub(crate) struct WatchDesk {
    /// Into the loop, once it runs.
    signals: OnceLock<mpsc::Sender<Signal>>,
    /// Set when a notice found the channel full: the loop reads every panel it would watch.
    overflow: Arc<AtomicBool>,
    /// Another program has the focus.
    background: AtomicBool,
    /// By [`PanelId::index`]: watched by the system, and nothing reported since it was read.
    quiet: [AtomicBool; 2],
    /// By [`PanelId::index`]: how often the watcher read the panel.
    reads: [AtomicU64; 2],
}

impl WatchDesk {
    /// Passes `signal` to the loop without waiting; a full channel sets the overflow mark.
    /// Whether the loop runs.
    fn send(&self, signal: Signal) -> bool {
        let Some(signals) = self.signals.get() else {
            return false;
        };
        if signals.try_send(signal).is_err() {
            self.overflow.store(true, Ordering::Release);
        }
        true
    }
}

impl Engine {
    /// Starts the file watcher's loop; a second call changes nothing. The window calls it once
    /// the database is attached; a headless engine never does.
    pub fn start_watching(&self) {
        let (sender, receiver) = mpsc::channel(CHANNEL);
        if self.inner.watch.signals.set(sender.clone()).is_ok() {
            run::spawn(self, sender, receiver);
        }
    }

    /// The window came to the front (`true`) or another program took the focus; a layout
    /// fact the window reports, not a user action: no command, no event.
    pub fn set_foreground(&self, front: bool) {
        self.inner.watch.background.store(!front, Ordering::Release);
        self.inner.watch.send(Signal::Foreground);
    }

    /// Whether `panel` is watched by the system and nothing was reported since it was last
    /// read: coming back to the window need not read it (RC-100-69).
    #[must_use]
    pub fn watch_quiet(&self, panel: PanelId) -> bool {
        self.inner.watch.quiet[panel.index()].load(Ordering::Acquire)
    }

    /// A local shell is back at its prompt (`commands/terminal_watch.rs`); whether the loop
    /// runs to read the panels.
    pub(crate) fn watch_prompt(&self) -> bool {
        self.inner.watch.send(Signal::Prompt)
    }

    /// `panel.toggle_watch` (Total Commander's `cm_SwitchWatchDirs`): `panels.watch` switched
    /// and stored; the loop follows the setting.
    pub(crate) async fn toggle_watch(&self) -> Result<Outcome, EngineError> {
        let on = self.setting_on(WATCH_KEY).await;
        self.set_setting(WATCH_KEY, &Value::Bool(!on)).await
    }

    /// How often the watcher read `panel` (tests count them).
    #[cfg(test)]
    pub(crate) fn watch_reads(&self, panel: PanelId) -> u64 {
        self.inner.watch.reads[panel.index()].load(Ordering::Acquire)
    }
}

/// Whether `event` may change what is watched: a panel showed another folder, its terminal or
/// another tab, or the drive list (a network drive) changed. A cursor or a selection does not.
fn moves_panels(event: &EngineEvent) -> bool {
    matches!(
        event,
        EngineEvent::Panel {
            change: PanelChange::Navigated | PanelChange::View | PanelChange::Tabs,
            ..
        } | EngineEvent::Drives
    )
}
