use super::*;
use crate::test_support::{Content, Repeated, TestSource, VecSink, source};
use crate::text::MAX_LINE_BYTES;
use crate::types::{FailureKind, TokenClass};

/// One record line of the large documents, with escapes, non-ASCII, every class.
const RECORD: &str = r#"  {"id": 12345, "name": "caf\u00e9 \"quoted\"", "tags": ["a", "b"], "ok": true, "n": null, "v": -1.5e+3},"#;

fn open<C: Content>(src: TestSource<C>, format: Format) -> Viewer<TestSource<C>> {
    Viewer::open(src, format).expect("the document opens").0
}

fn text_of<C: Content>(viewer: &mut Viewer<TestSource<C>>) -> String {
    let mut sink = VecSink::default();
    assert_eq!(viewer.text(&mut sink).expect("text"), "utf-8");
    String::from_utf8(sink.0).expect("the text is UTF-8")
}

/// The tokens line `index` of `lines` must get: its first [`MAX_LINE_BYTES`], a key when a
/// colon starts the next line that is not blank.
fn expected(lines: &[&str], index: usize) -> Vec<Token> {
    let bytes = lines[index].as_bytes();
    let capped = bytes.len() > MAX_LINE_BYTES;
    let following = lines
        .iter()
        .skip(index + 1)
        .take(4)
        .map(|line| &line.as_bytes()[..line.len().min(256)]);
    let colon = !capped && tokens::colon_follows(following);
    tokens::tokenize_line(&bytes[..bytes.len().min(MAX_LINE_BYTES)], capped, colon)
}

#[test]
fn a_document_offers_highlighted_then_tree() {
    let (viewer, document) =
        Viewer::open(source("{}"), Format::from_setting(Some("pretty"))).expect("opens");
    assert_eq!(document.outputs, vec![Output::Highlighted, Output::Tree]);
    assert_eq!(document, viewer.document());
    assert_eq!(Format::from_setting(Some("as_is")), Format::AsIs);
    assert_eq!(Format::from_setting(Some("other")), Format::AsIs);
    assert_eq!(Format::from_setting(None), Format::AsIs);
}

#[test]
fn probe_looks_for_the_start_of_a_value() {
    for text in ["{}", "\u{feff} \n[1]", "\"s\"", "-1", "true", "null"] {
        assert!(probe(&mut source(text)), "{text}");
    }
    for text in ["", "   ", "<xml/>", "# title", "\u{feff}"] {
        assert!(!probe(&mut source(text)), "{text}");
    }
}

#[test]
fn crlf_lines_are_tokenized_without_their_carriage_return() {
    let text = "{\r\n  \"a\": 1,\r\n  \"b\": \"x\"\r\n}\r\n";
    let mut viewer = open(source(text), Format::AsIs);
    assert_eq!(viewer.line_count(), 4);
    let tokens = viewer.tokens(0, 10).expect("tokens");
    assert_eq!(tokens.len(), 4);
    let line = &tokens[1];
    assert_eq!(line[0].class, TokenClass::Key);
    let last = line.last().expect("a comma");
    assert_eq!(last.start + last.len, "  \"a\": 1,".len() as u32);
    assert_eq!(text_of(&mut viewer), text);
}

#[test]
fn a_byte_order_mark_is_not_part_of_the_text() {
    let mut viewer = open(source("\u{feff}{\"a\": 1}\n"), Format::AsIs);
    assert_eq!(viewer.line_count(), 1);
    let tokens = viewer.tokens(0, 1).expect("tokens");
    assert_eq!(
        (tokens[0][0].start, tokens[0][1].class),
        (0, TokenClass::Key)
    );
    assert_eq!(text_of(&mut viewer), "{\"a\": 1}\n");
    assert_eq!(viewer.parse_error(), None);
}

#[test]
fn pretty_text_is_indented_two_spaces_a_level() {
    let mut viewer = open(source(r#"{"a":[1,2,{}],"b":"c"}"#), Format::Pretty);
    assert_eq!(
        text_of(&mut viewer),
        "{\n  \"a\": [\n    1,\n    2,\n    {}\n  ],\n  \"b\": \"c\"\n}"
    );
    assert_eq!(viewer.line_count(), 8);
    let tokens = viewer.tokens(1, 1).expect("tokens");
    assert_eq!(tokens[0][0].class, TokenClass::Key);
    assert_eq!(tokens[0][0].start, 2);
}

/// The setting decides the text (RC-100-14): `pretty` indents a minified file, `as_is` shows it
/// as the file has it.
#[test]
fn the_format_setting_chooses_the_text() {
    let minified = r#"{"a":[1,2],"b":{"c":null}}"#;
    let mut as_is = open(source(minified), Format::from_setting(Some("as_is")));
    assert_eq!(as_is.line_count(), 1);
    assert_eq!(text_of(&mut as_is), minified);
    let mut pretty = open(source(minified), Format::from_setting(Some("pretty")));
    assert_eq!(
        text_of(&mut pretty),
        "{\n  \"a\": [\n    1,\n    2\n  ],\n  \"b\": {\n    \"c\": null\n  }\n}"
    );
    assert_eq!(pretty.line_count(), 9);
}

#[test]
fn a_colon_on_a_later_line_makes_a_key() {
    let mut viewer = open(source("{\n  \"a\"\n\n  : 1\n}"), Format::AsIs);
    assert_eq!(
        viewer.tokens(1, 1).expect("tokens")[0][0].class,
        TokenClass::Key
    );
}

#[test]
fn only_lines_that_exist_get_tokens() {
    let mut viewer = open(source("[\n1\n]"), Format::AsIs);
    assert_eq!(viewer.tokens(1, 10).expect("tokens").len(), 2);
    assert!(viewer.tokens(3, 10).expect("tokens").is_empty());
    assert!(viewer.tokens(0, 0).expect("tokens").is_empty());
    let mut empty = open(source(""), Format::AsIs);
    assert_eq!(empty.line_count(), 0);
    assert!(empty.tokens(0, 1).expect("tokens").is_empty());
}

/// Records, a 100 KiB line, more records: checkpoints by lines and by bytes, a capped line.
fn mixed_document() -> String {
    let mut text = String::from("[\n");
    for index in 0..1500 {
        text.push_str(&RECORD.replace("12345", &index.to_string()));
        text.push('\n');
    }
    text.push_str(&format!("  \"{}\",\n", "y".repeat(100 * 1024)));
    for _ in 0..1500 {
        text.push_str(RECORD);
        text.push('\n');
    }
    text.push_str("  0\n]\n");
    text
}

fn tokens_match_the_full_text(format: Format) {
    let mut viewer = open(source(&mixed_document()), format);
    let text = text_of(&mut viewer);
    let mut lines: Vec<&str> = text.split('\n').collect();
    if text.ends_with('\n') {
        lines.pop();
    }
    assert_eq!(viewer.line_count(), lines.len() as u64);
    for index in (0..lines.len())
        .step_by(7)
        .chain(lines.len() - 3..lines.len())
    {
        let got = viewer.tokens(index as u32, 1).expect("tokens");
        assert_eq!(got, vec![expected(&lines, index)], "line {index}");
    }
    for first in [0, 63, 64, 1499, 1500, 1501, 2000, lines.len() - 20] {
        let got = viewer.tokens(first as u32, 50).expect("tokens");
        let want: Vec<Vec<Token>> = (first..lines.len().min(first + 50))
            .map(|index| expected(&lines, index))
            .collect();
        assert_eq!(got, want, "lines from {first}");
    }
}

#[test]
fn as_is_tokens_from_any_checkpoint_match_the_whole_text() {
    tokens_match_the_full_text(Format::AsIs);
}

#[test]
fn pretty_tokens_from_any_checkpoint_match_the_whole_text() {
    tokens_match_the_full_text(Format::Pretty);
}

#[test]
fn moving_on_cancels_the_open() {
    let mut src = TestSource::new(mixed_document().into_bytes());
    src.stop_after = Some(1);
    let failure = Viewer::open(src, Format::AsIs)
        .err()
        .expect("the open stops");
    assert_eq!(failure.kind, FailureKind::Cancelled);
}

fn large(size: u64) -> Repeated {
    let body = format!("{RECORD}\n").into_bytes();
    Repeated {
        head: b"{\"items\": [\n".to_vec(),
        times: size / body.len() as u64 + 1,
        body,
        tail: b"  null\n]}\n".to_vec(),
    }
}

/// `open` reads the file once in windows of at most 1 MiB and keeps its index within budget;
/// 50 lines in the middle and a page of the tree there read a few windows, not the file.
fn large_document_stays_bounded(size: u64, format: Format) {
    let content = large(size);
    let (size, times) = (content.size(), content.times);
    let mut viewer = open(TestSource::new(content), format);
    assert!(viewer.src.largest_read <= OPEN_CHUNK);
    assert!(viewer.src.bytes_read <= size + 3, "one pass");
    assert!(
        viewer.index_bytes() <= INDEX_BUDGET_BYTES,
        "{}",
        viewer.index_bytes()
    );
    assert_eq!(viewer.parse_error(), None);
    if format == Format::AsIs {
        assert_eq!(viewer.line_count(), times + 3);
    }

    let middle = (viewer.line_count() / 2) as u32;
    viewer.src.reset_counts();
    let tokens = viewer.tokens(middle, 50).expect("tokens");
    assert_eq!(tokens.len(), 50);
    assert!(
        viewer.src.bytes_read <= 1 << 20,
        "{}",
        viewer.src.bytes_read
    );
    if format == Format::AsIs {
        let record = tokens::tokenize_line(RECORD.as_bytes(), false, false);
        assert!(tokens.iter().all(|line| *line == record));
    }

    viewer.src.reset_counts();
    let top = viewer.children(None).expect("top");
    let items = viewer.children(Some(top[0].id)).expect("ranges");
    let middle_range = &items[items.len() / 2];
    let page = viewer
        .children(Some(middle_range.id))
        .expect("ranges or items");
    assert!(!page.is_empty());
    assert!(
        viewer.src.bytes_read <= 1 << 20,
        "{}",
        viewer.src.bytes_read
    );
    assert!(viewer.index_bytes() <= INDEX_BUDGET_BYTES);
}

#[test]
fn an_8_mib_document_stays_bounded() {
    large_document_stays_bounded(8 << 20, Format::AsIs);
    large_document_stays_bounded(8 << 20, Format::Pretty);
}

/// The acceptance case: 100 MiB. Left out of the normal run, a minute and more in a debug build;
/// `cargo test --release -- --ignored` runs it.
#[test]
#[ignore = "100 MiB through a debug build is slow; run with --ignored"]
fn a_100_mib_document_stays_bounded() {
    large_document_stays_bounded(100 << 20, Format::AsIs);
    large_document_stays_bounded(100 << 20, Format::Pretty);
}

/// The tokens of `text`, by line, each on character boundaries of its line.
fn assert_on_boundaries(text: &str, tokens: &[Vec<Token>]) {
    for (line, spans) in text.split('\n').zip(tokens) {
        for token in spans {
            let end = (token.start + token.len) as usize;
            assert!(
                line.is_char_boundary(token.start as usize) && line.is_char_boundary(end),
                "{token:?} in {line:?}"
            );
        }
    }
}

#[test]
fn invalid_utf8_is_written_as_u_fffd_and_the_tokens_follow_it() {
    // Invalid bytes before a token, in a key and in a string (a cut character): the text is the
    // file with U+FFFD in their place, and the tokens colour the same characters as there.
    let broken = b"{\xFF \"k\xFFey\": \"v\xE2\x82\",\n \"n\": [1, \"\xC3\"]}\n".to_vec();
    let clean = "{\u{fffd} \"k\u{fffd}ey\": \"v\u{fffd}\",\n \"n\": [1, \"\u{fffd}\"]}\n";
    for format in [Format::AsIs, Format::Pretty] {
        let mut viewer = open(TestSource::new(broken.clone()), format);
        let mut twin = open(source(clean), format);
        let text = text_of(&mut viewer);
        assert_eq!(text, text_of(&mut twin), "{format:?}");
        let tokens = viewer.tokens(0, 100).expect("tokens");
        assert_eq!(tokens, twin.tokens(0, 100).expect("tokens"), "{format:?}");
        assert!(
            tokens
                .iter()
                .flatten()
                .any(|token| token.class == TokenClass::Key)
        );
        assert_on_boundaries(&text, &tokens);
    }
}
