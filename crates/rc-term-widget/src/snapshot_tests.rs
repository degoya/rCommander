use alacritty_terminal::{
    Term, event::VoidListener, term::Config, term::cell::Flags, vte::ansi::Processor,
};
use rc_term::TermSize;

use super::{Snapshot, glyph, screen_row};
use crate::{ColorPalette, theme::Theme};

fn term_with(bytes: &[u8]) -> Term<VoidListener> {
    let size = TermSize { rows: 3, cols: 10 };
    let mut term = Term::new(Config::default(), &size, VoidListener);
    let mut parser: Processor = Processor::new();
    parser.advance(&mut term, bytes);
    term
}

#[test]
fn a_glyph_keeps_its_combining_marks_and_spacers_draw_nothing() {
    assert_eq!(glyph('a', None, Flags::empty()), "a");
    assert_eq!(glyph('e', Some(&['\u{301}']), Flags::empty()), "e\u{301}");
    assert_eq!(glyph(' ', None, Flags::empty()), "");
    assert_eq!(glyph('\t', None, Flags::empty()), "");
    assert_eq!(glyph(' ', Some(&['\u{301}']), Flags::empty()), " \u{301}");
    assert_eq!(glyph('x', None, Flags::HIDDEN), "");
    assert_eq!(glyph(' ', None, Flags::WIDE_CHAR_SPACER), "");
    assert_eq!(glyph(' ', None, Flags::LEADING_WIDE_CHAR_SPACER), "");
    assert_eq!(glyph('\u{4e2d}', None, Flags::WIDE_CHAR), "\u{4e2d}");
}

#[test]
fn a_row_in_the_history_is_on_screen_once_scrolled_back() {
    assert_eq!(screen_row(0, 0), Some(0));
    assert_eq!(screen_row(-2, 3), Some(1));
    assert_eq!(screen_row(-4, 3), None);
}

/// Only cells that draw something are copied; a wide glyph is one cell of two columns, its
/// combining marks stay with their glyph, and the cursor's cell is kept even when blank.
#[test]
fn the_snapshot_holds_what_the_frame_draws() {
    let term = term_with("ab\u{4e2d}c e\u{301}".as_bytes());
    let snapshot = Snapshot::take(&Theme::new(&ColorPalette::default()), &term);
    let cells: Vec<(usize, usize, &str, bool)> = snapshot
        .cells
        .iter()
        .map(|cell| (cell.row, cell.column, cell.text.as_str(), cell.is_wide()))
        .collect();
    assert_eq!(
        cells,
        [
            (0, 0, "a", false),
            (0, 1, "b", false),
            (0, 2, "\u{4e2d}", true),
            (0, 4, "c", false),
            (0, 6, "e\u{301}", false),
            (0, 7, "", false),
        ]
    );
    let cursor = snapshot.cursor.expect("a cursor");
    assert_eq!((cursor.row, cursor.column, cursor.wide), (0, 7, false));
}
