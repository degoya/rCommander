use super::*;
use crate::test_support::{Content, Repeated, TestSource, VecSink, source};
use crate::types::TokenClass::{self, Comment, Key, Keyword, Number, Operator, Punctuation};

/// One record of the large documents, six lines in cp850 (`#` becomes 0x81, a `u` umlaut): a
/// block over lines, a `^` continuation, delayed expansion, redirections.
const RECORD: &str = "if exist \"%~dp0file.txt\" (\r\n    echo found !n! ^\r\n      more & set /a n+=1\r\n) else (\r\n    echo m#ssing >> log.txt 2>&1\r\n)\r\n";

fn cp850(text: &str) -> Vec<u8> {
    text.bytes()
        .map(|byte| if byte == b'#' { 0x81 } else { byte })
        .collect()
}

fn open<C: Content>(src: TestSource<C>, encoding: Encoding) -> Viewer<TestSource<C>> {
    Viewer::open(src, encoding).expect("the document opens").0
}

fn text_of<C: Content>(viewer: &mut Viewer<TestSource<C>>) -> String {
    let mut sink = VecSink::default();
    assert_eq!(viewer.text(&mut sink).expect("text"), "utf-8");
    String::from_utf8(sink.0).expect("the text is UTF-8")
}

fn token(start: u32, len: u32, class: TokenClass) -> Token {
    Token { start, len, class }
}

#[test]
fn a_document_offers_highlighted_only() {
    let (viewer, document) = Viewer::open(source("cls\r\n"), Encoding::Auto).expect("opens");
    assert_eq!(document.outputs, vec![Output::Highlighted]);
    assert_eq!(document, viewer.document());
}

#[test]
fn probe_takes_text_and_refuses_binary() {
    for text in ["@echo off\r\n", "", "\u{feff}cls", "plain"] {
        assert!(probe(&mut source(text)), "{text}");
    }
    assert!(probe(&mut TestSource::new(cp850("echo m#ssing"))));
    assert!(!probe(&mut TestSource::new(b"MZ\0\x01".to_vec())));
}

#[test]
fn the_text_is_utf8_after_the_byte_order_mark_and_crlf_stays_out_of_the_tokens() {
    let mut viewer = open(source("\u{feff}@echo off\r\nrem x\r\n"), Encoding::Auto);
    assert_eq!(viewer.encoding(), Encoding::Utf8);
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(text_of(&mut viewer), "@echo off\r\nrem x\r\n");
    let tokens = viewer.tokens(0, 2).expect("tokens");
    assert_eq!(
        tokens,
        vec![
            vec![token(0, 1, Punctuation), token(1, 4, Keyword)],
            vec![token(0, 5, Comment)],
        ]
    );
}

#[test]
fn every_encoding_setting_reads_the_file_its_way() {
    let bytes = b"echo \x84\x9B\r\n".to_vec();
    for (setting, text) in [
        (Encoding::Auto, "echo \u{e4}\u{f8}\r\n"),
        (Encoding::Utf8, "echo \u{fffd}\u{fffd}\r\n"),
        (Encoding::Cp850, "echo \u{e4}\u{f8}\r\n"),
        (Encoding::Cp437, "echo \u{e4}\u{a2}\r\n"),
        (Encoding::Windows1252, "echo \u{201e}\u{203a}\r\n"),
    ] {
        let mut viewer = open(TestSource::new(bytes.clone()), setting);
        assert_eq!(text_of(&mut viewer), text, "{setting:?}");
        let tokens = viewer.tokens(0, 1).expect("tokens");
        assert_eq!(tokens, vec![vec![token(0, 4, Keyword)]], "{setting:?}");
    }
    assert_eq!(
        open(TestSource::new(bytes), Encoding::Auto).encoding(),
        Encoding::Cp850
    );
    // `auto` keeps valid UTF-8, with or without a byte-order mark.
    let utf8 = "echo \u{e4}\u{f6}\u{fc} \u{20ac}\n";
    let mut viewer = open(source(utf8), Encoding::Auto);
    assert_eq!(viewer.encoding(), Encoding::Utf8);
    assert_eq!(text_of(&mut viewer), utf8);
    // A byte-order mark read in a code page is three characters of it.
    let mut viewer = open(source("\u{feff}cls\n"), Encoding::Cp850);
    assert_eq!(viewer.encoding(), Encoding::Cp850);
    assert_eq!(text_of(&mut viewer), "\u{b4}\u{2557}\u{2510}cls\n");
}

#[test]
fn token_offsets_are_utf8_bytes_of_the_text() {
    let mut viewer = open(
        TestSource::new(b"echo \x84 %x%\r\nset \x94=1\r\n".to_vec()),
        Encoding::Cp850,
    );
    let text = text_of(&mut viewer);
    assert_eq!(text, "echo \u{e4} %x%\r\nset \u{f6}=1\r\n");
    let tokens = viewer.tokens(0, 2).expect("tokens");
    assert_eq!(
        tokens,
        vec![
            vec![token(0, 4, Keyword), token(8, 3, Key)],
            vec![token(0, 3, Keyword), token(7, 1, Number)],
        ]
    );
}

#[test]
fn only_lines_that_exist_get_tokens() {
    let mut viewer = open(source("cls\necho"), Encoding::Auto);
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(viewer.tokens(1, 10).expect("tokens").len(), 1);
    assert!(viewer.tokens(2, 10).expect("tokens").is_empty());
    assert!(viewer.tokens(0, 0).expect("tokens").is_empty());
    let mut empty = open(source(""), Encoding::Auto);
    assert_eq!(empty.line_count(), 0);
    assert!(empty.tokens(0, 1).expect("tokens").is_empty());
    assert_eq!(text_of(&mut empty), "");
}

/// Records, a 100 KiB line with a `^` past the lexed part, a line of more than 4096 variables,
/// more records: checkpoints by lines and by bytes, inside blocks and continuations.
fn mixed_document() -> Vec<u8> {
    let mut text = String::from("@echo off\r\nsetlocal enabledelayedexpansion\r\n");
    for _ in 0..300 {
        text.push_str(RECORD);
    }
    text.push_str(&format!("(echo {} ^\r\n", "x".repeat(100 * 1024)));
    text.push_str(&format!("echo {}\r\n)\r\n", "%a%".repeat(5000)));
    for _ in 0..300 {
        text.push_str(RECORD);
    }
    cp850(&text)
}

#[test]
fn tokens_from_any_checkpoint_match_the_whole_text() {
    let bytes = mixed_document();
    let mut viewer = open(TestSource::new(bytes), Encoding::Auto);
    assert_eq!(viewer.encoding(), Encoding::Cp850);
    let text = text_of(&mut viewer);
    let count = u32::try_from(viewer.line_count()).expect("a few thousand lines");
    assert_eq!(count as usize, text.lines().count());
    let whole = viewer.tokens(0, count).expect("every line");
    assert_eq!(whole.len(), count as usize);
    for (line, tokens) in text.lines().zip(&whole) {
        for token in tokens {
            let start = token.start as usize;
            assert!(line.get(start..start + token.len as usize).is_some());
        }
    }
    for index in (0..count).step_by(7).chain(count - 3..count) {
        let got = viewer.tokens(index, 1).expect("tokens");
        assert_eq!(got[0], whole[index as usize], "line {index}");
    }
    for first in [0, 63, 64, 65, 1801, 1802, 1803, 1900, count - 20] {
        let got = viewer.tokens(first, 50).expect("tokens");
        let end = (first as usize + 50).min(whole.len());
        assert_eq!(
            got,
            whole[first as usize..end].to_vec(),
            "lines from {first}"
        );
    }
    let class = |tokens: &[Token]| tokens.iter().map(|token| token.class).collect::<Vec<_>>();
    let line = |needle: &str| {
        text.lines()
            .position(|line| line.contains(needle))
            .expect("the line is there")
    };
    // A line continued from `echo` is still `echo`'s text up to the `&`.
    assert_eq!(
        class(&whole[line("more & set")]),
        vec![Operator, Keyword, Keyword, Number]
    );
    assert_eq!(whole[line("%a%%a%")].len(), tokens::MAX_TOKENS_PER_LINE);
    // The caret past the lexed part of the long line continues nothing.
    assert_eq!(class(&whole[line("%a%%a%")])[0], Keyword);
}

#[test]
fn moving_on_cancels_the_open() {
    let mut src = TestSource::new(mixed_document());
    src.stop_after = Some(1);
    let failure = Viewer::open(src, Encoding::Auto)
        .err()
        .expect("the open stops");
    assert_eq!(failure.kind, FailureKind::Cancelled);
}

fn large(size: u64) -> Repeated {
    let body = cp850(RECORD);
    Repeated {
        head: b"@echo off\r\n".to_vec(),
        times: size / body.len() as u64 + 1,
        body,
        tail: b"exit /b 0\r\n".to_vec(),
    }
}

/// `open` reads the file once in windows of at most 1 MiB and keeps its index within budget; 50
/// lines in the middle read a window or two, not the file.
fn large_document_stays_bounded(size: u64) {
    let content = large(size);
    let (size, times) = (content.size(), content.times);
    let mut viewer = open(TestSource::new(content), Encoding::Auto);
    assert!(viewer.src.largest_read <= OPEN_CHUNK);
    assert!(viewer.src.bytes_read <= size + 3, "one pass");
    assert!(
        viewer.index_bytes() <= INDEX_BUDGET_BYTES,
        "{}",
        viewer.index_bytes()
    );
    assert_eq!(viewer.encoding(), Encoding::Cp850);
    assert_eq!(viewer.line_count(), times * 6 + 2);

    let middle = u32::try_from(viewer.line_count() / 2).expect("fewer lines than u32");
    viewer.src.reset_counts();
    let tokens = viewer.tokens(middle, 50).expect("tokens");
    assert_eq!(tokens.len(), 50);
    assert!(
        viewer.src.bytes_read <= 1 << 20,
        "{}",
        viewer.src.bytes_read
    );
    assert!(tokens.iter().all(|line| !line.is_empty()));
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
