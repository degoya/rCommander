use alacritty_terminal::{Term, event::VoidListener, term::Config, vte::ansi::Processor};

use super::{CursorPos, screen_text};
use crate::TermSize;

fn term_with(size: TermSize, scrollback: usize, bytes: &[u8]) -> Term<VoidListener> {
    let config = Config {
        scrolling_history: scrollback,
        ..Config::default()
    };
    let mut term = Term::new(config, &size, VoidListener);
    let mut parser: Processor = Processor::new();
    parser.advance(&mut term, bytes);
    term
}

#[test]
fn lines_are_trimmed_and_the_cursor_is_where_the_output_stopped() {
    let term = term_with(
        TermSize { rows: 4, cols: 20 },
        100,
        b"first   \r\nsecond\r\nthird",
    );
    let screen = screen_text(&term, 0);
    assert_eq!(screen.rows, 4);
    assert_eq!(screen.cols, 20);
    assert_eq!(screen.lines, ["first", "second", "third", ""]);
    assert!(screen.scrollback.is_empty());
    assert_eq!(
        screen.cursor,
        CursorPos {
            row: 2,
            col: 5,
            visible: true
        }
    );
}

#[test]
fn scrollback_is_limited_to_what_was_asked_and_what_exists() {
    let term = term_with(
        TermSize { rows: 2, cols: 10 },
        100,
        b"one\r\ntwo\r\nthree\r\nfour\r\nfive",
    );
    let all = screen_text(&term, 50);
    assert_eq!(all.scrollback, ["one", "two", "three"]);
    assert_eq!(all.lines, ["four", "five"]);
    let last = screen_text(&term, 1);
    assert_eq!(last.scrollback, ["three"]);
}

#[test]
fn the_history_limit_of_the_terminal_holds() {
    let term = term_with(TermSize { rows: 2, cols: 10 }, 1, b"a\r\nb\r\nc\r\nd");
    assert_eq!(screen_text(&term, 50).scrollback, ["b"]);
}

#[test]
fn a_wide_character_appears_once_and_colours_are_dropped() {
    let term = term_with(
        TermSize { rows: 2, cols: 10 },
        0,
        "\u{65e5}\u{672c} \x1b[31mred\x1b[0m".as_bytes(),
    );
    assert_eq!(screen_text(&term, 0).lines[0], "\u{65e5}\u{672c} red");
}

#[test]
fn a_hidden_cursor_is_reported() {
    let term = term_with(TermSize { rows: 2, cols: 10 }, 0, b"x\x1b[?25l");
    assert!(!screen_text(&term, 0).cursor.visible);
}
