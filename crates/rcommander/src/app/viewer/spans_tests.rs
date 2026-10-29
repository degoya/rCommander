//! Placing token spans on the line as the Lister shows it, the cache of the page, and the
//! highlighter that paints them.

use iced::advanced::text::Highlighter;
use rc_engine::viewer::TextLine;

use super::{MAX_CACHED_LINES, SpanHighlighter, Spans, place};
use crate::app::viewer::document::{Token, TokenClass};

fn token(start: u32, len: u32, class: TokenClass) -> Token {
    Token { start, len, class }
}

fn lines(starts: impl IntoIterator<Item = u64>) -> Vec<TextLine> {
    starts
        .into_iter()
        .map(|offset| TextLine {
            offset,
            text: "key: value".to_owned(),
        })
        .collect()
}

#[test]
fn a_plain_line_keeps_its_spans() {
    let tokens = [
        token(0, 3, TokenClass::Key),
        token(5, 5, TokenClass::String),
    ];
    assert_eq!(
        place("key: value", &tokens, 8, 0),
        [(0..3, TokenClass::Key), (5..10, TokenClass::String)]
    );
}

#[test]
fn tabs_control_characters_and_the_cut_columns_move_the_spans() {
    // A tab at column 1 is seven spaces at width 8.
    let tabbed = place("a\tb", &[token(2, 1, TokenClass::Number)], 8, 0);
    assert_eq!(tabbed, [(8..9, TokenClass::Number)]);
    // A control character of two bytes shows as one dot.
    let control = place("\u{85}x", &[token(2, 1, TokenClass::Literal)], 8, 0);
    assert_eq!(control, [(1..2, TokenClass::Literal)]);
    // Two columns scrolled off: a token cut at the left keeps its rest, one off screen goes.
    let cut = place(
        "abcdef",
        &[
            token(0, 1, TokenClass::Comment),
            token(1, 3, TokenClass::Key),
        ],
        8,
        2,
    );
    assert_eq!(cut, [(0..2, TokenClass::Key)]);
    // Without a tab width a tab is a control character: one dot.
    let dot = place("\tz", &[token(1, 1, TokenClass::Function)], 0, 0);
    assert_eq!(dot, [(1..2, TokenClass::Function)]);
}

#[test]
fn a_multibyte_character_counts_its_bytes() {
    let line = "\u{e4}\u{f6}=\u{1F600}";
    let tokens = [
        token(0, 4, TokenClass::Key),
        token(4, 1, TokenClass::Operator),
        token(5, 4, TokenClass::String),
        token(6, 1, TokenClass::Invalid),
    ];
    assert_eq!(
        place(line, &tokens, 8, 0),
        [
            (0..4, TokenClass::Key),
            (4..5, TokenClass::Operator),
            (5..9, TokenClass::String),
        ]
    );
}

#[test]
fn the_page_asks_each_line_once_and_a_failed_answer_is_not_asked_again() {
    let mut spans = Spans::default();
    spans.set_page(lines([0, 11, 22]));
    assert_eq!(spans.wanted(), [0, 11, 22]);
    assert!(spans.wanted().is_empty(), "asked already");
    spans.arrived(&[0, 11, 22], Err("plugin_failed"));
    spans.refresh(true, 8, 0);
    assert!(spans.shown().line(0).is_empty());
    assert!(spans.wanted().is_empty(), "kept without tokens");
    // An answer nobody asked for is dropped.
    spans.arrived(&[99], Ok(vec![vec![token(0, 1, TokenClass::Key)]]));
    spans.set_page(lines([99]));
    assert_eq!(spans.wanted(), [99]);
}

#[test]
fn the_cache_shrinks_to_the_page_past_its_limit() {
    let mut spans = Spans::default();
    let every: Vec<u64> = (0..=MAX_CACHED_LINES as u64).collect();
    spans.set_page(lines(every.clone()));
    let wanted = spans.wanted();
    spans.set_page(lines([5, 6]));
    spans.arrived(&wanted, Ok(Vec::new()));
    assert_eq!(spans.cache.len(), 2, "only the page's lines stay");
    assert!(spans.wanted().is_empty());
}

#[test]
fn the_highlighter_paints_line_by_line_and_drops_what_does_not_fit_the_line() {
    let mut spans = Spans::default();
    spans.set_page(lines([0, 11]));
    let wanted = spans.wanted();
    let answer = vec![
        vec![token(0, 3, TokenClass::Key)],
        vec![token(5, 5, TokenClass::String)],
    ];
    spans.arrived(&wanted, Ok(answer));
    spans.refresh(true, 8, 0);
    let mut painter = SpanHighlighter::new(spans.shown());
    let first: Vec<_> = painter.highlight_line("key: value").collect();
    assert_eq!(first, [(0..3, TokenClass::Key)]);
    // The editor's line is shorter than the page's: the span past it is dropped.
    assert!(painter.highlight_line("key").next().is_none());
    assert_eq!(painter.current_line(), 2);
    painter.change_line(1);
    let again: Vec<_> = painter.highlight_line("key: value").collect();
    assert_eq!(again, [(5..10, TokenClass::String)]);

    let before = spans.shown().clone();
    spans.refresh(true, 8, 0);
    assert!(*spans.shown() != before, "new spans repaint");
    painter.update(spans.shown());
    assert_eq!(painter.current_line(), 0);
    spans.refresh(false, 8, 0);
    assert!(spans.shown().line(0).is_empty(), "off: nothing painted");
}
