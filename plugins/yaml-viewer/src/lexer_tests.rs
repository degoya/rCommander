use super::*;
use crate::reader::SliceBytes;

type Starts = Vec<(u64, u64, LineState)>;

fn lex(text: &str) -> (Vec<Lexeme>, Starts) {
    let mut lexer = Lexer::new(SliceBytes::new(text.as_bytes()));
    let mut lexemes = Vec::new();
    let mut starts = Vec::new();
    while let Some(lexeme) = lexer
        .next_with(&mut |line, offset, state| starts.push((line, offset, state)))
        .expect("a slice cannot fail")
    {
        lexemes.push(lexeme);
    }
    (lexemes, starts)
}

fn kinds(text: &str) -> Vec<Kind> {
    lex(text).0.iter().map(|lexeme| lexeme.kind).collect()
}

#[test]
fn lexemes_carry_their_line_column_in_characters_and_byte_column() {
    let (lexemes, _) = lex("\u{e9}\u{e9}: x\n  - y");
    let positions: Vec<(Kind, u64, u64, u32)> = lexemes
        .iter()
        .map(|lexeme| (lexeme.kind, lexeme.line, lexeme.column, lexeme.col))
        .collect();
    assert_eq!(
        positions,
        vec![
            (Kind::Plain, 1, 1, 0),
            (Kind::Colon, 1, 3, 4),
            (Kind::Plain, 1, 5, 6),
            (Kind::Dash, 2, 3, 2),
            (Kind::Plain, 2, 5, 4),
        ]
    );
}

#[test]
fn every_line_start_carries_what_is_open() {
    let text = "a: |\n  one\nb: \"two\n  three\"\nc: [1,\n  2]\nd: 4\n";
    let (_, starts) = lex(text);
    let states: Vec<(u64, LineState)> = starts.iter().map(|start| (start.0, start.2)).collect();
    let block = |indent| LineState {
        block: Some(Block { min: 1, indent }),
        ..LineState::default()
    };
    assert_eq!(
        states,
        vec![
            (1, block(None)),
            (2, block(Some(2))),
            (
                3,
                LineState {
                    quote: b'"',
                    ..LineState::default()
                }
            ),
            (4, LineState::default()),
            (
                5,
                LineState {
                    flow: 1,
                    ..LineState::default()
                }
            ),
            (6, LineState::default()),
            (7, LineState::default()),
        ]
    );
    // Every line start is reported where it is.
    assert!(starts.iter().all(|start| {
        let offset = usize::try_from(start.1).expect("small");
        text.as_bytes().get(offset - 1) == Some(&b'\n')
    }));
}

#[test]
fn a_block_scalar_ends_at_a_line_indented_less() {
    assert_eq!(
        kinds("- |2\n    kept\n   also\n x: 1\n"),
        vec![
            Kind::Dash,
            Kind::BlockHeader,
            Kind::BlockText,
            Kind::BlockText,
            Kind::Plain,
            Kind::Colon,
            Kind::Plain,
        ]
    );
    // At the top level the content may start in column 0; a document marker still ends it.
    assert_eq!(
        kinds("--- >\ntext\n---\n"),
        vec![
            Kind::DocStart,
            Kind::BlockHeader,
            Kind::BlockText,
            Kind::DocStart,
        ]
    );
}

#[test]
fn indicators_need_a_blank_after_them() {
    assert_eq!(
        kinds("-1: a:b\n- ?x\n"),
        vec![
            Kind::Plain,
            Kind::Colon,
            Kind::Plain,
            Kind::Dash,
            Kind::Plain,
        ]
    );
    assert_eq!(
        kinds("{\"a\":1, b: [c, d]}"),
        vec![
            Kind::FlowOpen,
            Kind::Quoted,
            Kind::Colon,
            Kind::Plain,
            Kind::Comma,
            Kind::Plain,
            Kind::Colon,
            Kind::FlowOpen,
            Kind::Plain,
            Kind::Comma,
            Kind::Plain,
            Kind::FlowClose,
            Kind::FlowClose,
        ]
    );
}

#[test]
fn a_plain_scalar_leaves_the_blanks_and_comment_after_it_out() {
    let text = "key: two words   # note\n";
    let (lexemes, _) = lex(text);
    let spans: Vec<(&str, Kind)> = lexemes
        .iter()
        .map(|lexeme| {
            (
                &text[lexeme.start as usize..lexeme.end as usize],
                lexeme.kind,
            )
        })
        .collect();
    assert_eq!(
        spans,
        vec![
            ("key", Kind::Plain),
            (":", Kind::Colon),
            ("two words", Kind::Plain),
            ("# note", Kind::Comment),
        ]
    );
}

#[test]
fn problems_are_invalid_lexemes() {
    assert_eq!(
        kinds("a:\n\tb: 1\n"),
        vec![
            Kind::Plain,
            Kind::Colon,
            Kind::Invalid(Problem::TabIndent),
            Kind::Plain,
            Kind::Colon,
            Kind::Plain,
        ]
    );
    assert_eq!(
        kinds("- \"\\q\"\n- @x\n- ]\n- [|]"),
        vec![
            Kind::Dash,
            Kind::Invalid(Problem::BadEscape),
            Kind::Dash,
            Kind::Invalid(Problem::Reserved),
            Kind::Dash,
            Kind::Invalid(Problem::StrayClose),
            Kind::Dash,
            Kind::FlowOpen,
            Kind::Invalid(Problem::BlockInFlow),
            Kind::FlowClose,
        ]
    );
}
