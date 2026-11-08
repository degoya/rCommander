//! Glyphs into lines of text: a PDF places each glyph where it is drawn and says nothing of words
//! or lines, so the text is put together from the glyphs' positions in the order the page shows
//! them. A glyph lower or higher than the one before by more than half its size starts a line; a
//! gap wider than [`SPACE_GAP`] times its size, or a step back, is a space. Glyphs without a
//! Unicode value are left out. The page's text is capped at [`MAX_PAGE_BYTES`].

/// The gap between two glyph origins on a line, in units of the font size, that reads as a space:
/// wider than a letter's advance.
pub const SPACE_GAP: f64 = 0.9;
/// The most text one page yields; the rest of the page is left out.
pub const MAX_PAGE_BYTES: usize = 4 << 20;

/// The text of one page, built glyph by glyph.
#[derive(Debug, Default)]
pub struct Layout {
    text: String,
    /// The previous glyph's origin and size.
    last: Option<(f64, f64, f64)>,
    full: bool,
}

impl Layout {
    /// The glyph `text` drawn at `x`, `y` (PDF user space, `y` up) with the font size `size`.
    pub fn push(&mut self, x: f64, y: f64, size: f64, text: &str) {
        if self.full || text.is_empty() || !x.is_finite() || !y.is_finite() {
            return;
        }
        let size = if size.is_finite() && size > 0.0 {
            size
        } else {
            1.0
        };
        if let Some((last_x, last_y, last_size)) = self.last {
            let scale = size.max(last_size);
            let gap = x - last_x;
            if (y - last_y).abs() > scale * 0.5 {
                self.break_line();
            } else if (gap > scale * SPACE_GAP || gap < -scale * 0.5) && !self.ends_blank() {
                self.text.push(' ');
            }
        }
        self.last = Some((x, y, size));
        if self.text.len() + text.len() > MAX_PAGE_BYTES {
            self.full = true;
            return;
        }
        self.text
            .extend(text.chars().map(|c| if c.is_control() { ' ' } else { c }));
    }

    fn ends_blank(&self) -> bool {
        self.text.ends_with([' ', '\n']) || self.text.is_empty()
    }

    fn break_line(&mut self) {
        while self.text.ends_with(' ') {
            self.text.pop();
        }
        if !self.text.is_empty() {
            self.text.push('\n');
        }
    }

    /// The page's text, its lines without trailing blanks, ending in a line break unless empty.
    #[must_use]
    pub fn finish(mut self) -> String {
        self.break_line();
        self.text
    }
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
