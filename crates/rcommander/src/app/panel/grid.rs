//! The thumbnails' grid (RC-100-89) as the panel steps through it: tiles left to right in lines
//! of [`Pane::tiles`], `..` the first tile. The window takes every step here, in its own rows, so
//! the `..` tile is reached as in the other views; the engine's steps (`panel.move_cursor` from
//! MCP) count the same lines without it.

use rc_engine::{Command, CursorStep, CursorTarget, ListMode, PanelId};

use super::Pane;

impl Pane {
    /// Whether the panel shows the thumbnails.
    pub fn thumbnails(&self) -> bool {
        self.state.mode == ListMode::Thumbnails
    }

    /// The tiles of a line that fit, from the list's measured width.
    pub fn set_tiles(&mut self, tiles: usize) {
        self.tiles = tiles.max(1);
        self.ensure_visible();
    }

    /// The rows of one line: the tiles that fit in the thumbnails, one row in the other views.
    pub(super) fn line(&self) -> usize {
        if self.thumbnails() {
            self.tiles.max(1)
        } else {
            1
        }
    }

    /// Left and Right: a column back or on in the brief view, a tile in the thumbnails; nothing
    /// in the full view. In the thumbnails every other step too: a line of tiles up or down (none
    /// above the first line), a page of its lines less one, the first or the last tile. `..`
    /// counts as a row.
    pub(super) fn column_step(
        &mut self,
        panel: Option<PanelId>,
        step: CursorStep,
    ) -> Option<Command> {
        let grid = self.thumbnails();
        if !(grid || self.brief()) || self.state.entries.is_empty() {
            return None;
        }
        let row = self.cursor_row();
        let last = self.rows() - 1;
        let tiles = self.line();
        let page = self.page_rows.saturating_sub(1).max(1) * tiles;
        let target = match step {
            CursorStep::Left if grid => row.saturating_sub(1),
            CursorStep::Right if grid => (row + 1).min(last),
            CursorStep::Left => row.saturating_sub(self.page_rows),
            CursorStep::Right => (row + self.page_rows).min(last),
            CursorStep::Up => row.checked_sub(tiles).unwrap_or(row),
            CursorStep::Down => (row + tiles).min(last),
            CursorStep::PageUp => row.saturating_sub(page),
            CursorStep::PageDown => (row + page).min(last),
            CursorStep::First => 0,
            CursorStep::Last => last,
        };
        if target == row {
            return None;
        }
        self.on_parent = self.has_parent_row() && target == 0;
        self.parent_asked = self.on_parent;
        Some(match self.entry_index(target) {
            Some(index) if !self.on_parent => Command::SetCursor {
                panel,
                target: CursorTarget::Index(index),
            },
            _ => Command::MoveCursor {
                panel,
                step: CursorStep::First,
            },
        })
    }
}

#[cfg(test)]
#[path = "grid_tests.rs"]
mod tests;
