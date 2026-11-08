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

/// Whether a panel shows its file list, its terminal or a folder tree.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelView {
    /// The directory's entries.
    #[default]
    Files,
    /// A terminal in the panel's place; the window owns the shell.
    Terminal,
    /// The folder tree of the panel's drive in its place, Total Commander's Ctrl+F8 (RC-100-88):
    /// the other panel shows the folder under the tree's cursor. Never kept in a workspace.
    Tree,
}

/// How a file list lays out its entries: Total Commander's full and brief views, a column set,
/// the comments view (RC-100-90) and the thumbnails (RC-100-89).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListMode {
    /// One entry per line with name, extension, size and date (Ctrl+F2).
    #[default]
    Full,
    /// Names only, in several columns side by side (Ctrl+F1).
    Brief,
    /// One entry per line with the columns of the column set `set`; a set deleted since shows
    /// the full view.
    Custom { set: i64 },
    /// One entry per line with name, extension and comment (Ctrl+Shift+F2).
    Comments,
    /// A grid of square tiles, a picture's thumbnail or an icon over each name (Ctrl+Shift+F1,
    /// RC-100-89).
    Thumbnails,
}

impl ListMode {
    /// Whether the entries stand one per line with columns: every mode but the brief view and
    /// the thumbnails.
    #[must_use]
    pub fn has_columns(self) -> bool {
        !matches!(self, Self::Brief | Self::Thumbnails)
    }
}

/// Where a cursor stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursorView {
    pub index: usize,
    pub name: String,
}

/// A panel's state by its id; here beside [`PanelId`], to keep `lib.rs` under 500 lines.
impl crate::State {
    pub(crate) fn panel_mut(&mut self, id: PanelId) -> &mut crate::PanelState {
        match id {
            PanelId::Left => &mut self.left,
            PanelId::Right => &mut self.right,
        }
    }

    pub(crate) fn panel(&self, id: PanelId) -> &crate::PanelState {
        match id {
            PanelId::Left => &self.left,
            PanelId::Right => &self.right,
        }
    }
}
