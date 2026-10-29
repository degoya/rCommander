use super::*;
use crate::test_support::{Content, Repeated, TestSource, VecSink, source};
use crate::types::{FailureKind, TokenClass};

/// One record of the large documents: a sequence item with a mapping, a quoted string with
/// escapes and non-ASCII, a flow collection, a literal and a block scalar.
const RECORD: &str = "  - id: 12345\n    name: \"caf\\u00e9 \\\"quoted\\\" \u{e9}\"\n    tags: [a, b]\n    ok: true\n    note: |\n      line one\n      line two\n";

fn open<C: Content>(src: TestSource<C>) -> Viewer<TestSource<C>> {
    Viewer::open(src).expect("the document opens").0
}

fn text_of<C: Content>(viewer: &mut Viewer<TestSource<C>>) -> String {
    let mut sink = VecSink::default();
    assert_eq!(viewer.text(&mut sink).expect("text"), "utf-8");
    String::from_utf8(sink.0).expect("the text is UTF-8")
}

#[test]
fn a_document_offers_highlighted_then_tree() {
    let (viewer, document) = Viewer::open(source("a: 1\n")).expect("opens");
    assert_eq!(document.outputs, vec![Output::Highlighted, Output::Tree]);
    assert_eq!(document, viewer.document());
}

#[test]
fn probe_takes_text_and_refuses_binary() {
    for text in ["a: 1\n", "", "\u{feff}- x", "plain"] {
        assert!(probe(&mut source(text)), "{text}");
    }
    assert!(!probe(&mut TestSource::new(b"a: \0\x01".to_vec())));
}

#[test]
fn the_text_is_the_file_after_its_byte_order_mark() {
    let mut viewer = open(source("\u{feff}a: 1\r\nb: 2\r\n"));
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(text_of(&mut viewer), "a: 1\r\nb: 2\r\n");
    assert_eq!(viewer.parse_error(), None);
}

#[test]
fn only_lines_that_exist_get_tokens() {
    let mut viewer = open(source("a: 1\nb: 2"));
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(viewer.tokens(1, 10).expect("tokens").len(), 1);
    assert!(viewer.tokens(2, 10).expect("tokens").is_empty());
    assert!(viewer.tokens(0, 0).expect("tokens").is_empty());
    let mut empty = open(source(""));
    assert_eq!(empty.line_count(), 0);
    assert!(empty.tokens(0, 1).expect("tokens").is_empty());
}

/// Records, a 100 KiB line, a long block scalar, a flow collection over many lines, a quoted
/// scalar over many lines, more records: checkpoints by lines and by bytes, and in every state.
fn mixed_document() -> String {
    let mut text = String::from("items:\n");
    for index in 0..300 {
        text.push_str(&RECORD.replace("12345", &index.to_string()));
    }
    text.push_str(&format!("long: {}\n", "y".repeat(100 * 1024)));
    text.push_str("script: |\n");
    for index in 0..200 {
        text.push_str(&format!("  echo {index} # not a comment: [x\n"));
    }
    text.push_str("flow: [\n");
    for index in 0..200 {
        text.push_str(&format!("  {index}, \"s\", {{k: v}},\n"));
    }
    text.push_str("  end]\nquoted: \"first\n");
    for _ in 0..200 {
        text.push_str("  still # quoted: - [\n");
    }
    text.push_str("  last\"\nrest:\n");
    for _ in 0..300 {
        text.push_str(RECORD);
    }
    text
}

#[test]
fn tokens_from_any_checkpoint_match_the_whole_text() {
    let text = mixed_document();
    let mut viewer = open(source(&text));
    assert_eq!(viewer.parse_error(), None);
    let count = u32::try_from(viewer.line_count()).expect("a few thousand lines");
    assert_eq!(count as usize, text.lines().count());
    let whole = viewer.tokens(0, count).expect("every line");
    assert_eq!(whole.len(), count as usize);
    for index in (0..count).step_by(7).chain(count - 3..count) {
        let got = viewer.tokens(index, 1).expect("tokens");
        assert_eq!(got[0], whole[index as usize], "line {index}");
    }
    for first in [0, 63, 64, 2099, 2100, 2101, 2200, 2400, 2700, count - 20] {
        let got = viewer.tokens(first, 50).expect("tokens");
        let end = (first as usize + 50).min(whole.len());
        assert_eq!(
            got,
            whole[first as usize..end].to_vec(),
            "lines from {first}"
        );
    }
    // A line inside the block scalar, the flow collection and the quoted scalar.
    let line = |needle: &str| {
        text.lines()
            .position(|line| line.contains(needle))
            .expect("the line is there")
    };
    let class = |tokens: &[Token]| tokens.iter().map(|token| token.class).collect::<Vec<_>>();
    use crate::types::TokenClass::{Number, Punctuation};
    assert_eq!(class(&whole[line("echo 150 ")]), vec![TokenClass::String]);
    assert_eq!(
        class(&whole[line("  150, \"s\"")]),
        vec![
            Number,
            Punctuation,
            TokenClass::String,
            Punctuation,
            Punctuation,
            TokenClass::Key,
            Punctuation,
            Punctuation,
            Punctuation,
        ]
    );
    assert_eq!(
        class(&whole[line("still # quoted")]),
        vec![TokenClass::String]
    );
}

#[test]
fn moving_on_cancels_the_open() {
    let mut src = TestSource::new(mixed_document().into_bytes());
    src.stop_after = Some(1);
    let failure = Viewer::open(src).err().expect("the open stops");
    assert_eq!(failure.kind, FailureKind::Cancelled);
}

fn large(size: u64) -> Repeated {
    let body = RECORD.as_bytes().to_vec();
    Repeated {
        head: b"# generated\nitems:\n".to_vec(),
        times: size / body.len() as u64 + 1,
        body,
        tail: b"end: true\n".to_vec(),
    }
}

/// `open` reads the file once in windows of at most 1 MiB and keeps its index within budget;
/// 50 lines in the middle and a page of the tree there read a few windows, not the file.
fn large_document_stays_bounded(size: u64) {
    let content = large(size);
    let (size, times) = (content.size(), content.times);
    let mut viewer = open(TestSource::new(content));
    assert!(viewer.src.largest_read <= OPEN_CHUNK);
    assert!(viewer.src.bytes_read <= size + 3, "one pass");
    assert!(
        viewer.index_bytes() <= INDEX_BUDGET_BYTES,
        "{}",
        viewer.index_bytes()
    );
    assert_eq!(viewer.parse_error(), None);
    assert_eq!(viewer.line_count(), times * 7 + 3);

    let middle = (viewer.line_count() / 2) as u32;
    viewer.src.reset_counts();
    let tokens = viewer.tokens(middle, 50).expect("tokens");
    assert_eq!(tokens.len(), 50);
    assert!(
        viewer.src.bytes_read <= 1 << 20,
        "{}",
        viewer.src.bytes_read
    );
    assert!(tokens.iter().any(|line| !line.is_empty()));

    viewer.src.reset_counts();
    let top = viewer.children(None).expect("top");
    assert_eq!(top.len(), 2);
    assert_eq!(top[0].value, Some(format!("[{times}]")));
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
    large_document_stays_bounded(8 << 20);
}

/// The acceptance case: 100 MiB. Left out of the normal run, a minute and more in a debug build;
/// `cargo test --release -- --ignored` runs it.
#[test]
#[ignore = "100 MiB through a debug build is slow; run with --ignored"]
fn a_100_mib_document_stays_bounded() {
    large_document_stays_bounded(100 << 20);
}

#[test]
fn invalid_utf8_is_written_as_u_fffd_and_the_tokens_follow_it() {
    // Invalid bytes in a key, a quoted string, a comment and a cut character in a plain scalar:
    // the text is the file with U+FFFD in their place, and the tokens colour the same characters.
    let broken = b"k\xFFey: \"v\xFF\" # c\xFF\nlist:\n  - \xE2\x82 x\n  - 12\n".to_vec();
    let clean = "k\u{fffd}ey: \"v\u{fffd}\" # c\u{fffd}\nlist:\n  - \u{fffd} x\n  - 12\n";
    let mut viewer = open(TestSource::new(broken));
    let mut twin = open(source(clean));
    let text = text_of(&mut viewer);
    assert_eq!(text, text_of(&mut twin));
    let tokens = viewer.tokens(0, 100).expect("tokens");
    assert_eq!(tokens, twin.tokens(0, 100).expect("tokens"));
    assert_eq!(tokens[0][0].class, TokenClass::Key);
    for (line, spans) in text.split('\n').zip(&tokens) {
        for token in spans {
            let end = (token.start + token.len) as usize;
            assert!(
                line.is_char_boundary(token.start as usize) && line.is_char_boundary(end),
                "{token:?} in {line:?}"
            );
        }
    }
}
