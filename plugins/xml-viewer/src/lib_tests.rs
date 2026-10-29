use super::*;
use crate::lexer::Mode;
use crate::test_support::{Content, Repeated, TestSource, VecSink, source};
use crate::text::MAX_LINE_BYTES;
use crate::tokens::{end_mode, tokenize_line};
use crate::types::{FailureKind, TokenClass};

/// One record line of the large documents: attributes, entities, a comment, CDATA, every class.
const RECORD: &str = r#"  <item id="12345" name="caf&#233; &quot;q&quot;"><tag>a &amp; b</tag><ok/><!-- c --><![CDATA[x<y]]></item>"#;

fn open<C: Content>(src: TestSource<C>, format: Format) -> Viewer<TestSource<C>> {
    Viewer::open(src, format).expect("the document opens").0
}

fn text_of<C: Content>(viewer: &mut Viewer<TestSource<C>>) -> String {
    let mut sink = VecSink::default();
    assert_eq!(viewer.text(&mut sink).expect("text"), "utf-8");
    String::from_utf8(sink.0).expect("the text is UTF-8")
}

/// The tokens every line of `lines` must get: its first [`MAX_LINE_BYTES`], lexed from the mode
/// the whole line before ended in.
fn expected(lines: &[&str]) -> Vec<Vec<Token>> {
    let mut mode = Mode::Content;
    lines
        .iter()
        .map(|line| {
            let bytes = line.as_bytes();
            let capped = bytes.len() > MAX_LINE_BYTES;
            let kept = &bytes[..bytes.len().min(MAX_LINE_BYTES)];
            let tokens = tokenize_line(kept, mode, capped).0;
            mode = end_mode(bytes, mode);
            tokens
        })
        .collect()
}

#[test]
fn a_document_offers_highlighted_then_tree() {
    let (viewer, document) =
        Viewer::open(source("<a/>"), Format::from_setting(Some("pretty"))).expect("opens");
    assert_eq!(document.outputs, vec![Output::Highlighted, Output::Tree]);
    assert_eq!(document, viewer.document());
    assert_eq!(Format::from_setting(Some("as_is")), Format::AsIs);
    assert_eq!(Format::from_setting(Some("other")), Format::AsIs);
    assert_eq!(Format::from_setting(None), Format::AsIs);
}

#[test]
fn probe_looks_for_markup_first() {
    for text in ["<a/>", "\u{feff} \n<?xml?>", "<!-- c -->"] {
        assert!(probe(&mut source(text)), "{text}");
    }
    for text in ["", "   ", "{\"a\": 1}", "# title", "\u{feff}", "text <a/>"] {
        assert!(!probe(&mut source(text)), "{text}");
    }
}

#[test]
fn crlf_lines_are_tokenized_without_their_carriage_return() {
    let text = "<a>\r\n  <b x=\"1\"/>\r\n  <!-- c\r\n  d -->\r\n</a>\r\n";
    let mut viewer = open(source(text), Format::AsIs);
    assert_eq!(viewer.line_count(), 5);
    assert_eq!(viewer.parse_error(), None);
    let tokens = viewer.tokens(0, 10).expect("tokens");
    assert_eq!(tokens.len(), 5);
    let last = tokens[1].last().expect("a `/>`");
    assert_eq!(last.start + last.len, "  <b x=\"1\"/>".len() as u32);
    assert_eq!(
        tokens[2],
        vec![Token {
            start: 2,
            len: 6,
            class: TokenClass::Comment
        }]
    );
    assert_eq!(
        tokens[3],
        vec![Token {
            start: 0,
            len: 7,
            class: TokenClass::Comment
        }]
    );
    assert_eq!(text_of(&mut viewer), text);
}

#[test]
fn a_byte_order_mark_is_not_part_of_the_text() {
    let mut viewer = open(source("\u{feff}<a b=\"1\"/>\n"), Format::AsIs);
    assert_eq!(viewer.line_count(), 1);
    let tokens = viewer.tokens(0, 1).expect("tokens");
    assert_eq!(
        (tokens[0][0].start, tokens[0][1].class),
        (0, TokenClass::TypeName)
    );
    assert_eq!(text_of(&mut viewer), "<a b=\"1\"/>\n");
    assert_eq!(viewer.parse_error(), None);
    assert_eq!(viewer.children(None).expect("top")[0].label, "<a b=\"1\"/>");
}

#[test]
fn pretty_text_is_an_element_a_line() {
    let mut viewer = open(source("<a><b>x</b><c d='1'/></a>"), Format::Pretty);
    assert_eq!(text_of(&mut viewer), "<a>\n  <b>x</b>\n  <c d='1'/>\n</a>");
    assert_eq!(viewer.line_count(), 4);
    let tokens = viewer.tokens(2, 1).expect("tokens");
    assert_eq!(
        (tokens[0][0].start, tokens[0][1].class),
        (2, TokenClass::TypeName)
    );
}

#[test]
fn a_line_inside_a_comment_is_coloured_as_one() {
    let mut viewer = open(source("<a>\n<!--\n x\n-->\n</a>"), Format::AsIs);
    assert_eq!(
        viewer.tokens(2, 1).expect("tokens"),
        vec![vec![Token {
            start: 0,
            len: 2,
            class: TokenClass::Comment
        }]]
    );
}

#[test]
fn only_lines_that_exist_get_tokens() {
    let mut viewer = open(source("<a>\n<b/>\n</a>"), Format::AsIs);
    assert_eq!(viewer.tokens(1, 10).expect("tokens").len(), 2);
    assert!(viewer.tokens(3, 10).expect("tokens").is_empty());
    assert!(viewer.tokens(0, 0).expect("tokens").is_empty());
    let mut empty = open(source(""), Format::AsIs);
    assert_eq!(empty.line_count(), 0);
    assert!(empty.tokens(0, 1).expect("tokens").is_empty());
}

/// Records with comments and values over lines, a 100 KiB line, a 100 KiB line that ends inside
/// a comment, more records: checkpoints by lines, by bytes and pinned, capped lines.
fn mixed_document() -> String {
    let mut text = String::from("<?xml version=\"1.0\"?>\n<root>\n");
    for index in 0..1500 {
        text.push_str(&RECORD.replace("12345", &index.to_string()));
        text.push('\n');
        if index % 50 == 0 {
            text.push_str(&format!(
                "  <!-- note {index}\n       spans\n       lines -->\n"
            ));
        }
        if index % 50 == 25 {
            text.push_str("  <m v=\"a\n  b\" w='c'/>\n");
        }
    }
    text.push_str(&format!("  <long>{}</long>\n", "y".repeat(100 * 1024)));
    text.push_str(&format!(
        "  <!-- {}\n  still a comment -->\n",
        "z".repeat(100 * 1024)
    ));
    for _ in 0..1500 {
        text.push_str(RECORD);
        text.push('\n');
    }
    text.push_str("</root>\n");
    text
}

fn tokens_match_the_full_text(format: Format) {
    let mut viewer = open(source(&mixed_document()), format);
    assert_eq!(viewer.parse_error(), None);
    let text = text_of(&mut viewer);
    let mut lines: Vec<&str> = text.split('\n').collect();
    if text.ends_with('\n') {
        lines.pop();
    }
    assert_eq!(viewer.line_count(), lines.len() as u64);
    let want = expected(&lines);
    for index in (0..lines.len())
        .step_by(7)
        .chain(lines.len() - 3..lines.len())
    {
        let got = viewer.tokens(index as u32, 1).expect("tokens");
        assert_eq!(got, vec![want[index].clone()], "line {index}");
    }
    let long = lines
        .iter()
        .position(|line| line.len() > MAX_LINE_BYTES)
        .expect("a long line");
    for first in [
        0,
        63,
        64,
        1499,
        1500,
        1501,
        long - 1,
        long,
        long + 1,
        lines.len() - 20,
    ] {
        let got = viewer.tokens(first as u32, 50).expect("tokens");
        let end = lines.len().min(first + 50);
        assert_eq!(got, want[first..end].to_vec(), "lines from {first}");
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
fn a_long_line_without_a_checkpoint_after_it_is_read_through() {
    let text = format!("<!-- {}\nstill -->\n<a/>\n", "z".repeat(100 * 1024));
    let mut src = source(&text);
    // An index that kept no checkpoint after the long line.
    let mut lines = lines::LineIndex::new(0);
    lines.finish(3);
    let read = text::as_is_lines(&mut src, &lines, 1, 2).expect("lines");
    assert_eq!(read.len(), 2);
    assert_eq!(read[0].mode, Mode::Comment);
    assert_eq!(read[0].bytes, b"still -->");
    assert_eq!(read[1].mode, Mode::Content);
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
        head: b"<?xml version=\"1.0\"?>\n<items>\n".to_vec(),
        times: size / body.len() as u64 + 1,
        body,
        tail: b"</items>\n".to_vec(),
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
        let record = tokenize_line(RECORD.as_bytes(), Mode::Content, false).0;
        assert!(tokens.iter().all(|line| *line == record));
    }

    viewer.src.reset_counts();
    let top = viewer.children(None).expect("top");
    assert_eq!(top.len(), 1);
    let ranges = viewer.children(Some(top[0].id)).expect("ranges");
    let middle_range = &ranges[ranges.len() / 2];
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

#[test]
fn invalid_utf8_is_written_as_u_fffd_and_the_tokens_follow_it() {
    // Invalid bytes in a name, a value, text, a comment and a cut character before a tag: the
    // text is the file with U+FFFD in their place, and the tokens colour the same characters.
    let broken = b"<r a\xFF=\"v\xFF\">t\xFFx<!-- c\xFF -->\n<b>\xE2\x82</b></r>\n".to_vec();
    let clean = "<r a\u{fffd}=\"v\u{fffd}\">t\u{fffd}x<!-- c\u{fffd} -->\n<b>\u{fffd}</b></r>\n";
    for format in [Format::AsIs, Format::Pretty] {
        let mut viewer = open(TestSource::new(broken.clone()), format);
        let mut twin = open(source(clean), format);
        let text = text_of(&mut viewer);
        assert_eq!(text, text_of(&mut twin), "{format:?}");
        let tokens = viewer.tokens(0, 100).expect("tokens");
        assert_eq!(tokens, twin.tokens(0, 100).expect("tokens"), "{format:?}");
        assert!(tokens.iter().flatten().count() > 0);
        for (line, spans) in text.split('\n').zip(&tokens) {
            for token in spans {
                let end = (token.start + token.len) as usize;
                assert!(
                    line.is_char_boundary(token.start as usize) && line.is_char_boundary(end),
                    "{format:?} {token:?} in {line:?}"
                );
            }
        }
    }
}
