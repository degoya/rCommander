//! The screen and the scrollback of a terminal as plain text, for MCP and tests.

use alacritty_terminal::{
    Term,
    grid::Dimensions,
    index::{Column, Line},
    term::{
        TermMode,
        cell::{Cell, Flags},
    },
};
use serde::{Deserialize, Serialize};

/// What a terminal shows, as text. Lines carry no trailing blanks; a wide character appears
/// once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenText {
    pub rows: u16,
    pub cols: u16,
    /// Lines that scrolled off the top, oldest first; as many as were asked for and exist.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scrollback: Vec<String>,
    /// The screen, top to bottom, one entry per row.
    pub lines: Vec<String>,
    pub cursor: CursorPos,
}

/// Where the cursor stands on the screen, counted from 0 at the top left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursorPos {
    pub row: u16,
    pub col: u16,
    /// Whether the program shows it.
    pub visible: bool,
}

/// The live screen of `term` (not the part a viewer has scrolled to) and up to `scrollback`
/// lines of history above it.
#[must_use]
pub fn screen_text<T>(term: &Term<T>, scrollback: usize) -> ScreenText {
    let grid = term.grid();
    let screen_lines = grid.screen_lines();
    let columns = grid.columns();
    let history = scrollback.min(grid.history_size());
    let row_text = |line: i32| {
        let row = &grid[Line(line)];
        let mut text = String::with_capacity(columns);
        for column in 0..columns {
            push_cell(&mut text, &row[Column(column)]);
        }
        text.truncate(text.trim_end().len());
        text
    };
    let first_history = -i32::try_from(history).unwrap_or(i32::MAX);
    let point = grid.cursor.point;
    ScreenText {
        rows: to_u16(screen_lines),
        cols: to_u16(columns),
        scrollback: (first_history..0).map(row_text).collect(),
        lines: (0..i32::try_from(screen_lines).unwrap_or(i32::MAX))
            .map(row_text)
            .collect(),
        cursor: CursorPos {
            row: u16::try_from(point.line.0).unwrap_or(0),
            col: to_u16(point.column.0),
            visible: term.mode().contains(TermMode::SHOW_CURSOR),
        },
    }
}

fn push_cell(text: &mut String, cell: &Cell) {
    if cell
        .flags
        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
    {
        return;
    }
    text.push(cell.c);
    if let Some(combining) = cell.zerowidth() {
        text.extend(combining);
    }
}

fn to_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

#[cfg(test)]
#[path = "screen_tests.rs"]
mod tests;
