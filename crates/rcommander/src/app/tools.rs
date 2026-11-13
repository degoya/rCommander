//! The windows of RC-100-98: "Select duplicate files" (Num+ over a duplicate result in a panel,
//! Total Commander's dialog, modal) and the disk-usage window (Ctrl+Shift+L, rCommander's own,
//! modeless like find files). Each opens in a window of its own while `appearance.dialog_windows`
//! is on, else as a card over the main window.

mod disk_usage;
pub(super) mod disk_usage_view;
mod select_duplicates;
pub(super) mod select_duplicates_view;

use rc_engine::EngineEvent;

pub use self::{
    disk_usage::{DiskUsageWindow, UsageMsg},
    select_duplicates::{SelMsg, SelectDuplicates},
};
use super::{App, Effect, Message};

/// The two windows' state.
#[derive(Debug, Default)]
pub struct ToolsUi {
    /// "Select duplicate files": open, or closed with the last choice kept for the session.
    pub select: Option<SelectDuplicates>,
    /// The disk-usage window, while it is open.
    pub usage: Option<DiskUsageWindow>,
    /// The number of the last disk-usage walk asked for, over every window of the session.
    usage_generation: u64,
}

impl ToolsUi {
    /// The number of the next disk-usage walk: never one an earlier window had, so a walk that
    /// starts after its window closed is told from the one a new window waits for.
    fn next_usage(&mut self) -> u64 {
        self.usage_generation += 1;
        self.usage_generation
    }

    /// Whether "Select duplicate files" is open.
    pub fn select_open(&self) -> bool {
        self.select.as_ref().is_some_and(|dialog| dialog.open)
    }
}

/// What the two windows' controls say.
#[derive(Debug, Clone)]
pub enum ToolsMsg {
    Select(SelMsg),
    Usage(UsageMsg),
}

impl App {
    pub(super) fn on_tools(&mut self, message: ToolsMsg) -> Vec<Effect> {
        match message {
            ToolsMsg::Select(message) => self.on_select_duplicates(message),
            ToolsMsg::Usage(message) => self.on_disk_usage(message),
        }
    }

    /// Ctrl+Shift+L opens the disk-usage window; while it is open it takes the keys and clicks
    /// that reach it, as find files does.
    pub(super) fn tools_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        if let Message::Engine(EngineEvent::DiskUsage { panel }) = message {
            return Some(self.open_disk_usage(*panel));
        }
        self.tools.usage.as_ref()?;
        match message {
            Message::Key(chord) => Some(self.disk_usage_key(chord, false)),
            Message::KeyCaptured(chord) => Some(self.disk_usage_key(chord, true)),
            Message::Press => Some(Vec::new()),
            _ => None,
        }
    }
}
