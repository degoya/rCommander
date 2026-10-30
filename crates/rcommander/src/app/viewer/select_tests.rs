use iced::{
    Background, Border, Color,
    advanced::mouse::click::Kind,
    widget::markdown::{self, Highlight, Item},
};

use super::{Gesture, Selection, Span, Spot, blocks, lines, marked, word_at};

fn items(source: &str) -> Vec<Item> {
    markdown::parse(source).collect()
}

fn texts(spans: &[Span]) -> Vec<&str> {
    spans.iter().map(|span| span.text.as_ref()).collect()
}

fn span(text: &str) -> Span {
    Span::new(text.to_owned())
}

fn spot(block: usize, at: usize) -> Spot {
    Spot { block, at }
}

fn press(at: Spot, kind: Kind, extend: bool) -> Gesture {
    Gesture::Press {
        spot: at,
        extend,
        kind,
    }
}

fn three() -> Vec<String> {
    ["first line", "second", "third one"]
        .map(str::to_owned)
        .to_vec()
}

#[test]
fn the_blocks_are_numbered_in_the_order_the_preview_draws_them() {
    let source = "# Title\n\nOne  \ntwo\n\n- item a\n- item b\n\n\
                  | A | B |\n|---|---|\n| a1 | b1 |\n\n```\nlet x;\nlet y;\n```\n";
    let found = blocks(&items(source));
    // A hard break splits the paragraph; a table reads its header, then row by row.
    assert_eq!(
        found,
        [
            "Title", "One", "two", "item a", "item b", "A", "B", "a1", "b1", "let x;", "let y;"
        ]
    );
}

#[test]
fn dragging_across_blocks_copies_one_line_a_block() {
    let blocks = three();
    let mut selection = Selection::default();
    selection.apply(press(spot(0, 6), Kind::Single, false), &blocks);
    assert!(selection.dragging);
    selection.apply(Gesture::Drag(spot(2, 5)), &blocks);
    selection.apply(Gesture::Release, &blocks);
    assert!(!selection.dragging);
    assert_eq!(
        selection.text(&blocks).as_deref(),
        Some("line\nsecond\nthird")
    );
    assert_eq!(selection.range_in(1, 6), Some(0..6));
    assert_eq!(selection.range_in(2, 9), Some(0..5));
}

#[test]
fn a_selection_dragged_backwards_reads_forwards() {
    let blocks = three();
    let mut selection = Selection::default();
    selection.apply(press(spot(1, 3), Kind::Single, false), &blocks);
    selection.apply(Gesture::Drag(spot(0, 6)), &blocks);
    assert_eq!(selection.text(&blocks).as_deref(), Some("line\nsec"));
}

#[test]
fn a_move_after_the_release_changes_nothing() {
    let blocks = three();
    let mut selection = Selection::default();
    selection.apply(press(spot(0, 0), Kind::Single, false), &blocks);
    selection.apply(Gesture::Drag(spot(0, 5)), &blocks);
    selection.apply(Gesture::Release, &blocks);
    selection.apply(Gesture::Drag(spot(2, 3)), &blocks);
    assert_eq!(selection.text(&blocks).as_deref(), Some("first"));
}

#[test]
fn shift_click_extends_and_a_click_alone_selects_nothing() {
    let blocks = three();
    let mut selection = Selection::default();
    selection.apply(press(spot(0, 6), Kind::Single, false), &blocks);
    selection.apply(Gesture::Release, &blocks);
    assert_eq!(selection.text(&blocks), None);
    selection.apply(press(spot(1, 3), Kind::Single, true), &blocks);
    assert_eq!(selection.text(&blocks).as_deref(), Some("line\nsec"));
}

#[test]
fn a_double_click_takes_the_word_and_a_triple_click_the_block() {
    let blocks = three();
    let mut selection = Selection::default();
    selection.apply(press(spot(2, 7), Kind::Double, false), &blocks);
    assert_eq!(selection.text(&blocks).as_deref(), Some("one"));
    assert!(!selection.dragging);
    selection.apply(press(spot(0, 2), Kind::Triple, false), &blocks);
    assert_eq!(selection.text(&blocks).as_deref(), Some("first line"));
}

#[test]
fn select_all_takes_every_block() {
    let blocks = three();
    let mut selection = Selection::default();
    selection.all(&blocks);
    assert_eq!(
        selection.text(&blocks).as_deref(),
        Some("first line\nsecond\nthird one")
    );
    let mut nothing = Selection::default();
    nothing.all(&[]);
    assert_eq!(nothing.text(&[]), None);
}

#[test]
fn a_word_is_letters_digits_and_underscores() {
    assert_eq!(word_at("foo bar_2 baz", 5), 4..9);
    assert_eq!(word_at("foo bar", 3), 0..3);
    // Just after a word is still the word; on anything else, that one character.
    assert_eq!(word_at("a, b", 1), 0..1);
    assert_eq!(word_at("a , b", 2), 2..3);
    assert_eq!(word_at("gr\u{fc}\u{df}e dir", 2), 0..7);
    assert_eq!(word_at("", 4), 0..0);
}

#[test]
fn a_hard_break_starts_a_new_block() {
    let found = lines(&[span("one"), span("\n"), span("two\nthree")]);
    let found: Vec<Vec<&str>> = found.iter().map(|line| texts(line)).collect();
    assert_eq!(found, [vec!["one"], vec!["two"], vec!["three"]]);
}

#[test]
fn marking_splits_the_spans_and_keeps_the_padding_outside() {
    let highlight = Highlight {
        background: Background::Color(Color::BLACK),
        border: Border::default(),
    };
    let mut code = span("code");
    code.padding = iced::Padding::from(2.0);
    let spans = [span("plain "), code];
    let marked = marked(&spans, 3..8, highlight);
    assert_eq!(texts(&marked), ["pla", "in ", "co", "de"]);
    let lit: Vec<bool> = marked.iter().map(|span| span.highlight.is_some()).collect();
    assert_eq!(lit, [false, true, true, false]);
    assert_eq!(
        (marked[2].padding.left, marked[2].padding.right),
        (2.0, 0.0)
    );
    assert_eq!(
        (marked[3].padding.left, marked[3].padding.right),
        (0.0, 2.0)
    );
}
