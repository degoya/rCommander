//! Which panel, how it shows its folder and where its cursor stands: the small types every
//! command names.

use serde::{Deserialize, Serialize};

/// One of the two panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelId {
    Left,
    Right,
}

impl PanelId {
    /// Both panels, left first.
    pub const BOTH: [Self; 2] = [Self::Left, Self::Right];

    /// 0 for the left panel, 1 for the right one: an index into a pair.
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 1,
        }
    }

    /// The panel on the other side.
    #[must_use]
    pub fn other(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

/// Whether a panel shows its file list or its terminal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelView {
    /// The directory's entries.
    #[default]
    Files,
    /// A terminal in the panel's place; the window owns the shell.
    Terminal,
}

/// How a file list lays out its entries: Total Commander's full and brief views.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListMode {
    /// One entry per line with name, extension, size and date (Ctrl+F2).
    #[default]
    Full,
    /// Names only, in several columns side by side (Ctrl+F1).
    Brief,
}

/// Where a cursor stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursorView {
    pub index: usize,
    pub name: String,
}
