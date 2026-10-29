use super::*;
use crate::Viewer;
use crate::test_support::source;
use crate::types::TokenClass::{
    Comment, Invalid, Key, Keyword, Literal, Number, Punctuation, TypeName,
};

type Span = (String, TokenClass);

/// The text and class of every token of every line of `text`, which must sit on character
/// boundaries inside the line.
fn spans(text: &str) -> Vec<Vec<Span>> {
    let mut viewer = Viewer::open(source(text))
        .expect("a source in memory opens")
        .0;
    let count = u32::try_from(viewer.line_count()).expect("a few lines");
    let tokens = viewer.tokens(0, count).expect("tokens");
    let body = text.strip_prefix('\u{feff}').unwrap_or(text);
    let lines: Vec<&str> = body
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    assert_eq!(tokens.len(), count as usize);
    tokens
        .iter()
        .zip(&lines)
        .map(|(line_tokens, line)| {
            line_tokens
                .iter()
                .map(|token| {
                    let start = token.start as usize;
                    let end = start + token.len as usize;
                    assert!(line.is_char_boundary(start) && line.is_char_boundary(end));
                    (line[start..end].to_owned(), token.class)
                })
                .collect()
        })
        .collect()
}

fn span(text: &str, class: TokenClass) -> Span {
    (text.to_owned(), class)
}

fn text(text: &str) -> Span {
    span(text, TokenClass::String)
}

fn key(name: &str) -> [Span; 2] {
    [span(name, Key), span(":", Punctuation)]
}

/// `key` and `:`, then `rest`.
fn keyed(name: &str, rest: &[Span]) -> Vec<Span> {
    key(name).into_iter().chain(rest.iter().cloned()).collect()
}

#[test]
fn a_scalar_before_a_colon_is_a_key_and_after_it_a_value() {
    assert_eq!(
        spans("name: rc\nlist:\n  - a: 1\n    b: two words\n"),
        vec![
            keyed("name", &[]),
            keyed("list", &[]),
            [span("-", Punctuation)]
                .into_iter()
                .chain(keyed("a", &[span("1", Number)]))
                .collect(),
            keyed("b", &[]),
        ]
    );
}

#[test]
fn quoted_scalars_of_both_kinds_with_their_escapes() {
    assert_eq!(
        spans(
            "a: \"say \\\"hi\\\" \\u00e9 \u{e9}\"\nb: 'it''s'\nc: \"bad \\q\"\n\"quoted key\": 1\n"
        ),
        vec![
            keyed("a", &[text("\"say \\\"hi\\\" \\u00e9 \u{e9}\"")]),
            keyed("b", &[text("'it''s'")]),
            keyed("c", &[span("\"bad \\q\"", Invalid)]),
            vec![
                span("\"quoted key\"", Key),
                span(":", Punctuation),
                span("1", Number)
            ],
        ]
    );
}

#[test]
fn block_scalars_colour_their_lines_until_one_is_indented_less() {
    assert_eq!(
        spans(
            "run: |\n  echo one\n  # not a comment\n\n  echo two\nnext: >-\n    folded\nend: 1\n"
        ),
        vec![
            keyed("run", &[text("|")]),
            vec![text("echo one")],
            vec![text("# not a comment")],
            vec![],
            vec![text("echo two")],
            keyed("next", &[text(">-")]),
            vec![text("folded")],
            keyed("end", &[span("1", Number)]),
        ]
    );
}

#[test]
fn comments_start_after_a_blank_and_not_in_a_string() {
    assert_eq!(
        spans("# top\nkey: value # trailing\nurl: a#b\ntext: \"x # y\"\n"),
        vec![
            vec![span("# top", Comment)],
            keyed("key", &[span("# trailing", Comment)]),
            keyed("url", &[]),
            keyed("text", &[text("\"x # y\"")]),
        ]
    );
}

#[test]
fn numbers_and_literals_and_plain_text() {
    let items = [
        ("12", Some(Number)),
        ("-3.5e+2", Some(Number)),
        ("0x1F", Some(Number)),
        ("0o17", Some(Number)),
        (".inf", Some(Number)),
        (".nan", Some(Number)),
        ("true", Some(Literal)),
        ("~", Some(Literal)),
        ("null", Some(Literal)),
        ("yes", Some(Literal)),
        ("1.2.3", None),
        ("12abc", None),
        ("truthy", None),
    ];
    let text: String = items
        .iter()
        .map(|(item, _)| format!("- {item}\n"))
        .collect();
    let want: Vec<Vec<Span>> = items
        .iter()
        .map(|(item, class)| {
            let mut line = vec![span("-", Punctuation)];
            line.extend(class.map(|class| span(item, class)));
            line
        })
        .collect();
    assert_eq!(spans(&text), want);
}

#[test]
fn anchors_aliases_and_tags_are_type_names() {
    assert_eq!(
        spans("base: &base\n  a: 1\nuse: *base\nmerge:\n  <<: *base\ntyped: !!str 5\n"),
        vec![
            keyed("base", &[span("&base", TypeName)]),
            keyed("a", &[span("1", Number)]),
            keyed("use", &[span("*base", TypeName)]),
            keyed("merge", &[]),
            keyed("<<", &[span("*base", TypeName)]),
            keyed("typed", &[span("!!str", TypeName), span("5", Number)]),
        ]
    );
}

#[test]
fn flow_collections_over_one_line_and_several() {
    let p = |text: &str| span(text, Punctuation);
    assert_eq!(
        spans("list: [a, 'b', 1, {k: v}]\nmulti: [\n  x, \"y\",\n  2]\n"),
        vec![
            keyed(
                "list",
                &[
                    p("["),
                    p(","),
                    text("'b'"),
                    p(","),
                    span("1", Number),
                    p(","),
                    p("{"),
                    span("k", Key),
                    p(":"),
                    p("}"),
                    p("]"),
                ]
            ),
            keyed("multi", &[p("[")]),
            vec![p(","), text("\"y\""), p(",")],
            vec![span("2", Number), p("]")],
        ]
    );
}

#[test]
fn document_markers_and_directives_are_keywords() {
    assert_eq!(
        spans("%YAML 1.2\n---\na: 1\n...\n--- !!map\nb: 2\n"),
        vec![
            vec![span("%YAML 1.2", Keyword)],
            vec![span("---", Keyword)],
            keyed("a", &[span("1", Number)]),
            vec![span("...", Keyword)],
            vec![span("---", Keyword), span("!!map", TypeName)],
            keyed("b", &[span("2", Number)]),
        ]
    );
}

#[test]
fn crlf_and_a_byte_order_mark_stay_out_of_the_spans() {
    assert_eq!(
        spans("\u{feff}a: 1\r\nb: \"x\"  \r\nc: |\r\n  text  \r\n"),
        vec![
            keyed("a", &[span("1", Number)]),
            keyed("b", &[text("\"x\"")]),
            keyed("c", &[text("|")]),
            vec![text("text")],
        ]
    );
}

#[test]
fn a_tab_in_the_indentation_is_invalid() {
    assert_eq!(
        spans("a:\n\tb: 1\n"),
        vec![
            keyed("a", &[]),
            [span("\t", Invalid)]
                .into_iter()
                .chain(keyed("b", &[span("1", Number)]))
                .collect(),
        ]
    );
}

#[test]
fn a_line_gets_at_most_4096_tokens() {
    let line = format!("[{}1]", "1, ".repeat(3000));
    let mut viewer = Viewer::open(source(&line))
        .expect("a source in memory opens")
        .0;
    let tokens = viewer.tokens(0, 1).expect("tokens");
    assert_eq!(tokens[0].len(), MAX_TOKENS_PER_LINE);
    assert!(
        tokens[0]
            .windows(2)
            .all(|pair| pair[0].start + pair[0].len <= pair[1].start)
    );
    assert_eq!(scalar_class(b"0x"), None);
    assert_eq!(scalar_class(b"+.5"), Some(Number));
}
