use super::*;
use crate::reader::SliceBytes;

/// The kinds of every lexeme of `text`, lexed from the start.
fn kinds(text: &str) -> Vec<Kind> {
    let mut lexer = Lexer::new(SliceBytes::new(text.as_bytes()));
    let mut out = Vec::new();
    while let Some(lexeme) = lexer.next().expect("a slice reads") {
        out.push(lexeme.kind);
    }
    out
}

#[test]
fn a_tag_is_lexed_into_its_parts() {
    assert_eq!(
        kinds(r#"<a b="1" c='2'>x</a><d/>"#),
        vec![
            Kind::Open,
            Kind::Name,
            Kind::AttrName,
            Kind::Eq,
            Kind::Value { closed: true },
            Kind::AttrName,
            Kind::Eq,
            Kind::Value { closed: true },
            Kind::Close,
            Kind::Text,
            Kind::EndOpen,
            Kind::Name,
            Kind::EndClose,
            Kind::Open,
            Kind::Name,
            Kind::EmptyClose,
        ]
    );
}

#[test]
fn every_line_start_reports_the_mode_it_is_in() {
    let text = "<a\n b='x\ny'><!-- c\nd --><![CDATA[\n]]>\n<?pi\n?><!DOCTYPE [\n]>\n";
    let mut lexer = Lexer::new(SliceBytes::new(text.as_bytes()));
    let mut starts = Vec::new();
    while lexer
        .next_with(&mut |line, offset, mode| starts.push((line, offset, mode)))
        .expect("a slice reads")
        .is_some()
    {}
    let modes: Vec<Mode> = starts.iter().map(|(_, _, mode)| *mode).collect();
    assert_eq!(
        modes,
        vec![
            Mode::Tag,
            Mode::Value(b'\''),
            Mode::Comment,
            Mode::CData,
            Mode::Content,
            Mode::Pi,
            Mode::Doctype { depth: 1, quote: 0 },
            Mode::Content,
        ]
    );
    assert_eq!(starts[0], (1, 3, Mode::Tag));
    assert_eq!(lexer.mode(), Mode::Content);
}

#[test]
fn a_lexer_resumes_inside_a_construct() {
    let mut lexer = Lexer::resume(
        SliceBytes::new(b"  still a comment --> <b/>"),
        LexState::at(Mode::Comment),
    );
    let comment = lexer.next().expect("reads").expect("a piece");
    assert_eq!(comment.kind, Kind::Comment { closed: true });
    // The whitespace inside a comment is its own.
    assert_eq!(comment.start, 0);
    assert_eq!(lexer.mode(), Mode::Content);
    let open = lexer.next().expect("reads").expect("a tag");
    assert_eq!((open.kind, open.state.spaced), (Kind::Open, true));
}

#[test]
fn markup_interrupting_a_tag_or_value_ends_it() {
    assert_eq!(
        kinds("<a b=\"x<c>"),
        vec![
            Kind::Open,
            Kind::Name,
            Kind::AttrName,
            Kind::Eq,
            Kind::Value { closed: false },
            Kind::Open,
            Kind::Name,
            Kind::Close,
        ]
    );
}

#[test]
fn positions_count_characters_and_lines() {
    let mut lexer = Lexer::new(SliceBytes::new("<\u{e9}l\u{e9}>\n  <b>".as_bytes()));
    let mut all = Vec::new();
    while let Some(lexeme) = lexer.next().expect("reads") {
        all.push(lexeme);
    }
    let at: Vec<(u64, u64)> = all
        .iter()
        .map(|lexeme| (lexeme.line, lexeme.column))
        .collect();
    assert_eq!(at, vec![(1, 1), (1, 2), (1, 5), (2, 3), (2, 4), (2, 5)]);
    assert!(all[3].state.newline && all[3].state.spaced);
    assert!(!all[4].state.spaced);
}
