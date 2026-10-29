//! The screen into a canvas frame at the frame's origin: backgrounds batched per run of a
//! colour, the cursor, underlines, then the glyphs. From iced_term 0.8's `view.rs` (MIT); it
//! drew at window coordinates, which a clipping parent cut away.
//!
//! It draws a [`Snapshot`] taken under the terminal's lock, not the locked terminal itself. A
//! wide glyph is centred over its two cells and a combining mark drawn with the glyph it belongs
//! to; a plain ASCII glyph skips the advanced shaping only other text needs (RC-080-46).

use alacritty_terminal::{term::cell::Flags, vte::ansi::CursorShape};
use iced::{
    Color, Point, Size,
    alignment::Vertical,
    font::{Style as FontStyle, Weight as FontWeight},
    widget::canvas::{Frame, Path, Stroke, Text},
};
use iced_core::text::{Alignment, LineHeight, Shaping};

use crate::{
    Terminal,
    snapshot::{Cell, Snapshot},
};

/// Draws `snapshot` into `frame`; `focused` draws a filled cursor, a hollow one otherwise. A
/// cursor the program hid has the shape `Hidden` and is not in the snapshot.
pub(crate) fn screen(frame: &mut Frame, terminal: &Terminal, snapshot: &Snapshot, focused: bool) {
    let cell = terminal.cell();
    let font_size = terminal.font.size;
    let mut run = Run::default();
    for drawn in &snapshot.cells {
        let x = drawn.column as f32 * cell.width;
        let y = drawn.row as f32 * cell.height;
        let mut fg = drawn.fg;
        let bg = drawn.bg;

        // Backgrounds: the container paints the default one; a run of one colour is one
        // rectangle.
        if bg == snapshot.background {
            run.flush(frame, cell);
        } else if !run.extends(bg, x, y) {
            run.flush(frame, cell);
            run = Run {
                color: Some(bg),
                start: Point::new(x, y),
                width: cell.width,
            };
        } else {
            run.width += cell.width;
        }

        if drawn.flags.contains(Flags::UNDERLINE) {
            let bottom = y + cell.height - 1.0;
            let underline = Path::line(Point::new(x, bottom), Point::new(x + cell.width, bottom));
            frame.stroke(
                &underline,
                Stroke::default()
                    .with_width(font_size * 0.08)
                    .with_color(fg),
            );
        }

        if let Some(cursor) = snapshot
            .cursor
            .filter(|cursor| cursor.row == drawn.row && cursor.column == drawn.column)
        {
            run.flush(frame, cell);
            let width = if cursor.wide { 2.0 } else { 1.0 };
            let size = Size::new(cell.width * width, cell.height);
            draw_cursor(
                frame,
                Point::new(x, y),
                size,
                cursor.shape,
                focused,
                cursor.color,
            );
            if focused && cursor.shape == CursorShape::Block {
                fg = bg;
            }
        }

        if !drawn.text.is_empty() {
            draw_glyph(frame, terminal, drawn, Point::new(x, y), fg);
        }
    }
    run.flush(frame, cell);
}

/// The glyph of `drawn` at the cell `at`: centred in its cell, or over both cells of a wide one.
fn draw_glyph(frame: &mut Frame, terminal: &Terminal, drawn: &Cell, at: Point, fg: Color) {
    let cell = terminal.cell();
    let mut font = terminal.font.font_type;
    if drawn.flags.intersects(Flags::BOLD | Flags::DIM_BOLD) {
        font.weight = FontWeight::Bold;
    }
    if drawn.flags.contains(Flags::ITALIC) {
        font.style = FontStyle::Italic;
    }
    let cells = if drawn.is_wide() { 2.0 } else { 1.0 };
    frame.fill_text(Text {
        content: drawn.text.clone(),
        position: Point::new(at.x + cell.width * cells * 0.5, at.y + cell.height * 0.5),
        font,
        size: iced_core::Pixels(terminal.font.size),
        color: fg,
        align_x: Alignment::Center,
        align_y: Vertical::Center,
        shaping: shaping(&drawn.text),
        line_height: LineHeight::Relative(terminal.font.scale_factor),
        ..Default::default()
    });
}

/// Basic shaping for one printable ASCII character - the font's own glyph, no fallback or
/// ligature to look for - and advanced shaping for everything else (combining marks, scripts that
/// join, emoji, glyphs from a fallback font).
pub(crate) fn shaping(text: &str) -> Shaping {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_graphic() => Shaping::Basic,
        _ => Shaping::Advanced,
    }
}

/// The cursor at `at` over `cell` (two cells on a wide glyph): a block (an outline while the
/// terminal has no focus), a bar or a line.
fn draw_cursor(
    frame: &mut Frame,
    at: Point,
    cell: Size,
    shape: CursorShape,
    focused: bool,
    color: Color,
) {
    let thin = (cell.width / 8.0).max(1.0);
    match shape {
        CursorShape::Beam => {
            frame.fill(&Path::rectangle(at, Size::new(thin, cell.height)), color);
        }
        CursorShape::Underline => {
            let bottom = Point::new(at.x, at.y + cell.height - thin);
            frame.fill(&Path::rectangle(bottom, Size::new(cell.width, thin)), color);
        }
        CursorShape::Block if focused => {
            frame.fill(&Path::rectangle(at, cell), color);
        }
        _ => {
            let outline = Path::rectangle(
                Point::new(at.x + 0.5, at.y + 0.5),
                Size::new(cell.width - 1.0, cell.height - 1.0),
            );
            frame.stroke(
                &outline,
                Stroke::default().with_width(1.0).with_color(color),
            );
        }
    }
}

/// A run of cells with one background colour on one line, drawn as one rectangle.
#[derive(Default)]
struct Run {
    color: Option<Color>,
    start: Point,
    width: f32,
}

impl Run {
    /// Whether a cell of `color` at `x`, `y` continues the run.
    fn extends(&self, color: Color, x: f32, y: f32) -> bool {
        self.color == Some(color)
            && (self.start.y - y).abs() < f32::EPSILON
            && (self.start.x + self.width - x).abs() < f32::EPSILON
    }

    /// Draws the run and starts none.
    fn flush(&mut self, frame: &mut Frame, cell: Size) {
        if let Some(color) = self.color.take() {
            frame.fill(
                &Path::rectangle(self.start, Size::new(self.width, cell.height)),
                color,
            );
        }
    }
}

#[cfg(test)]
#[path = "draw_tests.rs"]
mod tests;
