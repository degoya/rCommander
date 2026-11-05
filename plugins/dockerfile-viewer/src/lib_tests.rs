use super::*;
use crate::test_support::{Content, Repeated, TestSource, VecSink, highlighted, source};
use crate::types::{FailureKind, TokenClass};

/// One record of the documents, ten lines: a stage, a continued `RUN`, a file heredoc, pairs, a
/// comment, a port and a blank line.
const RECORD: &str = "FROM alpine:3.20 AS base\nRUN apk add --no-cache curl \\\n    && echo \"ok\"\nCOPY <<EOF /etc/motd\nhello $USER\nEOF\nENV A=1 B=\"two\"\n# comment\nEXPOSE 80\n\n";

fn open<C: Content>(src: TestSource<C>) -> Viewer<TestSource<C>> {
    Viewer::open(src).expect("the document opens").0
}

fn text_of<C: Content>(viewer: &mut Viewer<TestSource<C>>) -> Vec<u8> {
    let mut sink = VecSink::default();
    assert_eq!(viewer.text(&mut sink).expect("text"), "utf-8");
    sink.0
}

#[test]
fn probe_takes_text_and_leaves_binary_files() {
    assert!(probe(&mut source("FROM alpine\n")));
    assert!(probe(&mut source("")));
    assert!(!probe(&mut TestSource::new(b"FROM\0\x01".to_vec())));
}

#[test]
fn a_document_offers_highlighted() {
    let (viewer, document) = Viewer::open(source("FROM a\n")).expect("opens");
    assert_eq!(document.outputs, vec![Output::Highlighted]);
    assert_eq!(document, viewer.document());
}

#[test]
fn the_text_is_the_file_after_its_byte_order_mark() {
    let mut viewer = open(source("\u{feff}FROM a\r\nRUN b\r\n"));
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(text_of(&mut viewer), b"FROM a\r\nRUN b\r\n");
    let tokens = viewer.tokens(0, 2).expect("tokens");
    assert_eq!(
        tokens[0],
        vec![
            Token {
                start: 0,
                len: 4,
                class: TokenClass::Keyword
            },
            Token {
                start: 5,
                len: 1,
                class: TokenClass::TypeName
            }
        ],
        "the carriage return stays out of the tokens"
    );
}

#[test]
fn bytes_that_are_not_utf8_keep_the_tokens_on_character_boundaries() {
    let lines = highlighted(b"RUN echo \xFF \"\xE2\x82\"\n");
    assert_eq!(
        lines[0],
        [
            (TokenClass::Keyword, "RUN".to_owned()),
            (TokenClass::Function, "echo".to_owned()),
            (TokenClass::String, "\"\u{FFFD}\"".to_owned())
        ]
    );
}

#[test]
fn only_lines_that_exist_get_tokens() {
    let mut viewer = open(source("FROM a\nRUN b"));
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(viewer.tokens(1, 10).expect("tokens").len(), 1);
    assert!(viewer.tokens(2, 10).expect("tokens").is_empty());
    assert!(viewer.tokens(0, 0).expect("tokens").is_empty());
    let mut empty = open(source(""));
    assert_eq!(empty.line_count(), 0);
    assert!(empty.tokens(0, 1).expect("tokens").is_empty());
}

/// Records, a 100 KiB line, a shell heredoc and a label continued over many lines with a quote
/// left open, more records: checkpoints by lines and by bytes, in every state a line can start
/// in.
fn mixed_document() -> String {
    let mut text = String::from("# syntax=docker/dockerfile:1\n");
    for _ in 0..300 {
        text.push_str(RECORD);
    }
    text.push_str(&format!("RUN echo \"{}\"\n", "y".repeat(100 * 1024)));
    text.push_str("RUN <<EOF\n");
    for index in 0..200 {
        text.push_str(&format!("  echo {index} && cd /x\n"));
    }
    text.push_str("EOF\nLABEL note=\"first \\\n");
    for _ in 0..200 {
        text.push_str("  still quoted $v \\\n");
    }
    text.push_str("  last\"\n");
    for _ in 0..300 {
        text.push_str(RECORD);
    }
    text
}

#[test]
fn tokens_from_any_checkpoint_match_the_whole_text() {
    let text = mixed_document();
    let mut viewer = open(source(&text));
    let count = u32::try_from(viewer.line_count()).expect("a few thousand lines");
    assert_eq!(count as usize, text.lines().count());
    let whole = viewer.tokens(0, count).expect("every line");
    assert_eq!(whole.len(), count as usize);
    for index in (0..count).step_by(7).chain(count - 3..count) {
        let got = viewer.tokens(index, 1).expect("tokens");
        assert_eq!(got[0], whole[index as usize], "line {index}");
    }
    for first in [0, 63, 64, 3001, 3002, 3100, 3300, 3500, count - 20] {
        let got = viewer.tokens(first, 50).expect("tokens");
        let end = (first as usize + 50).min(whole.len());
        assert_eq!(
            got,
            whole[first as usize..end].to_vec(),
            "lines from {first}"
        );
    }
    let line = |needle: &str| {
        text.lines()
            .position(|line| line.contains(needle))
            .expect("the line is there")
    };
    let class = |tokens: &[Token]| tokens.iter().map(|token| token.class).collect::<Vec<_>>();
    use crate::types::TokenClass::{Function, Key, Number, Operator, Punctuation};
    assert_eq!(
        class(&whole[line("echo 150 ")]),
        vec![Function, Number, Operator, Function]
    );
    assert_eq!(
        class(&whole[line("still quoted")]),
        vec![TokenClass::String, Key, TokenClass::String, Punctuation]
    );
    assert_eq!(class(&whole[line("hello $USER")]), vec![TokenClass::String]);
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
        head: b"# syntax=docker/dockerfile:1\n".to_vec(),
        times: size / body.len() as u64 + 1,
        body,
        tail: b"CMD [\"sh\"]\n".to_vec(),
    }
}

/// `open` reads the file once in windows of at most 1 MiB and keeps its index within budget;
/// 50 lines in the middle read a window or two, not the file.
#[test]
fn an_8_mib_document_stays_bounded() {
    let content = large(8 << 20);
    let (size, times) = (content.size(), content.times);
    let mut viewer = open(TestSource::new(content));
    assert!(viewer.src.largest_read <= OPEN_CHUNK);
    assert!(viewer.src.bytes_read <= size + 3, "one pass");
    assert!(
        viewer.index_bytes() <= INDEX_BUDGET_BYTES,
        "{}",
        viewer.index_bytes()
    );
    assert_eq!(viewer.line_count(), times * 10 + 2);

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
}

#[test]
fn the_text_of_a_large_file_is_written_in_windows() {
    let content = large(3 << 20);
    let size = content.size();
    let mut viewer = open(TestSource::new(content));
    viewer.src.reset_counts();
    // `VecSink` refuses a write over 1 MiB.
    let text = text_of(&mut viewer);
    assert_eq!(text.len() as u64, size);
    assert!(viewer.src.largest_read <= OPEN_CHUNK);
}
