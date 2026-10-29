//! What one frame draws, copied out of the terminal while it is locked: the cells that show
//! anything, their colours resolved, and the cursor. Drawing then holds no lock, so the session's
//! pump never waits for a frame to be drawn (RC-080-46).

use alacritty_terminal::{
    Term,
    event::EventListener,
    term::cell::Flags,
    vte::ansi::{Color as AnsiColor, CursorShape, NamedColor},
};
use iced::Color;

use crate::theme::Theme;

/// One cell that draws a background, a glyph, a line or the cursor.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cell {
    /// The row on screen, from the top.
    pub row: usize,
    pub column: usize,
    /// The glyph with the combining characters that follow it; empty for none.
    pub text: String,
    pub fg: Color,
    pub bg: Color,
    pub flags: Flags,
}

impl Cell {
    /// The glyph takes two cells (East Asian wide, most emoji).
    pub(crate) fn is_wide(&self) -> bool {
        self.flags.contains(Flags::WIDE_CHAR)
    }
}

/// The cursor, where the program shows it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Cursor {
    pub row: usize,
    pub column: usize,
    pub shape: CursorShape,
    pub color: Color,
    /// It stands on a wide glyph: two cells wide.
    pub wide: bool,
}

/// One frame's screen.
#[derive(Debug)]
pub(crate) struct Snapshot {
    pub cells: Vec<Cell>,
    pub cursor: Option<Cursor>,
    pub background: Color,
}

impl Snapshot {
    /// Copies what `term` shows; held under the terminal's lock, so it only copies.
    pub(crate) fn take<T: EventListener>(theme: &Theme, term: &Term<T>) -> Self {
        let content = term.renderable_content();
        let display_offset = content.display_offset;
        let background = theme.background();
        let selection = content.selection;
        let colors = content.colors;
        let cursor_point = content.cursor.point;
        let shape = content.cursor.shape;
        let mut cursor = None;
        let mut cells = Vec::new();
        for indexed in content.display_iter {
            let Some(row) = screen_row(indexed.point.line.0, display_offset) else {
                continue;
            };
            let column = indexed.point.column.0;
            let flags = indexed.cell.flags;
            let mut fg = theme.color(indexed.cell.fg, colors);
            let mut bg = theme.color(indexed.cell.bg, colors);
            if flags.intersects(Flags::DIM | Flags::DIM_BOLD) {
                fg.a *= 0.7;
            }
            if flags.contains(Flags::INVERSE)
                || selection.is_some_and(|range| range.contains(indexed.point))
            {
                std::mem::swap(&mut fg, &mut bg);
            }
            if indexed.point == cursor_point && shape != CursorShape::Hidden {
                cursor = Some(Cursor {
                    row,
                    column,
                    shape,
                    color: theme.color(AnsiColor::Named(NamedColor::Foreground), colors),
                    wide: flags.contains(Flags::WIDE_CHAR),
                });
            }
            let text = glyph(indexed.cell.c, indexed.cell.zerowidth(), flags);
            let underline = flags.contains(Flags::UNDERLINE);
            let on_cursor =
                cursor.is_some_and(|cursor: Cursor| (cursor.row, cursor.column) == (row, column));
            if text.is_empty() && !underline && bg == background && !on_cursor {
                // Nothing to draw: the container paints the default background.
                continue;
            }
            cells.push(Cell {
                row,
                column,
                text,
                fg,
                bg,
                flags,
            });
        }
        Self {
            cells,
            cursor,
            background,
        }
    }
}

/// The screen row of grid line `line` (negative in the history) scrolled back by `offset`.
pub(crate) fn screen_row(line: i32, offset: usize) -> Option<usize> {
    let row = i64::from(line) + i64::try_from(offset).ok()?;
    usize::try_from(row).ok()
}

/// What a cell shows as text: its character and the combining ones after it; nothing for a
/// blank, a tab, a hidden cell or the spacer behind a wide glyph.
pub(crate) fn glyph(c: char, zerowidth: Option<&[char]>, flags: Flags) -> String {
    let spacer = flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER);
    let marks = zerowidth.unwrap_or_default();
    let blank = (c == ' ' || c == '\t') && marks.is_empty();
    if blank || spacer || flags.contains(Flags::HIDDEN) {
        return String::new();
    }
    let mut text = String::with_capacity(4);
    // A combining mark on a blank combines with a space, never with a tab.
    text.push(if c == '\t' { ' ' } else { c });
    text.extend(marks);
    text
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
