use super::*;
use crate::test_support::{Content, Repeated, TestSource, VecSink, source, utf16le};
use crate::types::TokenClass::{self, Comment, Key, Keyword, Number, Operator, Punctuation};

/// One record of the large documents, nine lines in Windows-1252 (`~` becomes 0xFC, a `u`
/// umlaut): a block comment over two lines, a block with a heredoc whose interpolation and marker
/// are lines of their own, and a one-line block.
const RECORD: &str = "/* Gr~~e of\r\n   a block */\r\nresource \"a\" \"b\" {\r\n  data = <<EOT\r\n  x ${var.y}\r\nEOT\r\n  n = 0.5 # done\r\n}\r\nlocals { s = \"~\" }\r\n";
const RECORD_LINES: u64 = 9;

fn ansi(text: &str) -> Vec<u8> {
    text.bytes()
        .map(|byte| if byte == b'~' { 0xFC } else { byte })
        .collect()
}

fn open<C: Content>(src: TestSource<C>, encoding: Option<Encoding>) -> Viewer<TestSource<C>> {
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

const STRING: TokenClass = TokenClass::String;

#[test]
fn a_document_offers_highlighted_only() {
    let (viewer, document) = Viewer::open(source("a = 1\r\n"), None).expect("opens");
    assert_eq!(document.outputs, vec![Output::Highlighted]);
    assert_eq!(document, viewer.document());
}

#[test]
fn probe_takes_text_and_utf16_and_refuses_binary() {
    for text in ["a = 1\r\n", "", "\u{feff}# x"] {
        assert!(probe(&mut source(text)), "{text}");
    }
    assert!(probe(&mut TestSource::new(ansi("# Gr~~e"))));
    assert!(probe(&mut TestSource::new(utf16le("a = 1\r\n", true))));
    assert!(probe(&mut TestSource::new(utf16le("a = 1\r\n", false))));
    assert!(!probe(&mut TestSource::new(
        b"\x7fELF\x02\x01\x01\0\0\0\0\0\0\0\0\x03\0".to_vec()
    )));
}

#[test]
fn the_setting_names_an_encoding_or_auto() {
    assert_eq!(encoding_setting(None), None);
    assert_eq!(encoding_setting(Some("auto")), None);
    assert_eq!(encoding_setting(Some("utf-8")), Some(Encoding::Utf8));
    assert_eq!(encoding_setting(Some("utf-16le")), Some(Encoding::Utf16Le));
    assert_eq!(
        encoding_setting(Some("windows-1252")),
        Some(Encoding::Windows1252)
    );
    assert_eq!(encoding_setting(Some("klingon")), None);
}

#[test]
fn the_text_is_utf8_after_the_byte_order_mark_and_crlf_stays_out_of_the_tokens() {
    let mut viewer = open(source("\u{feff}a = 1 # now\r\nb {\r\n"), None);
    assert_eq!(viewer.encoding(), Encoding::Utf8);
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(text_of(&mut viewer), "a = 1 # now\r\nb {\r\n");
    assert_eq!(
        viewer.tokens(0, 2).expect("tokens"),
        vec![
            vec![
                token(0, 1, Key),
                token(2, 1, Operator),
                token(4, 1, Number),
                token(6, 5, Comment),
            ],
            vec![token(0, 1, Keyword), token(2, 1, Punctuation)],
        ]
    );
}

#[test]
fn utf16_and_windows_1252_are_read_as_utf8_text() {
    let script = "s = \"Gr\u{fc}\u{df}e\"\r\nx {\r\n";
    let mut viewer = open(TestSource::new(utf16le(script, true)), None);
    assert_eq!(viewer.encoding(), Encoding::Utf16Le);
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(text_of(&mut viewer), script);
    let first = vec![token(0, 1, Key), token(2, 1, Operator), token(4, 9, STRING)];
    assert_eq!(
        viewer.tokens(0, 2).expect("tokens"),
        vec![
            first.clone(),
            vec![token(0, 1, Keyword), token(2, 1, Punctuation)]
        ]
    );
    let mut viewer = open(TestSource::new(b"s = \"Gr\xfc\xdfe\"\r\n".to_vec()), None);
    assert_eq!(viewer.encoding(), Encoding::Windows1252);
    assert_eq!(text_of(&mut viewer), "s = \"Gr\u{fc}\u{df}e\"\r\n");
    assert_eq!(viewer.tokens(0, 1).expect("tokens"), vec![first]);
    // A byte-order mark is skipped only when it is the chosen encoding's.
    let utf8_bom = "\u{feff}a = 1\n";
    let mut viewer = open(source(utf8_bom), Some(Encoding::Utf8));
    assert_eq!(text_of(&mut viewer), "a = 1\n");
    let mut viewer = open(source(utf8_bom), Some(Encoding::Windows1252));
    assert_eq!(text_of(&mut viewer), "\u{ef}\u{bb}\u{bf}a = 1\n");
}

#[test]
fn only_lines_that_exist_get_tokens() {
    let mut viewer = open(source("a = 1\nb = 2"), None);
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(viewer.tokens(1, 10).expect("tokens").len(), 1);
    assert!(viewer.tokens(2, 10).expect("tokens").is_empty());
    assert!(viewer.tokens(0, 0).expect("tokens").is_empty());
    let mut empty = open(source(""), None);
    assert_eq!(empty.line_count(), 0);
    assert!(empty.tokens(0, 1).expect("tokens").is_empty());
    assert_eq!(text_of(&mut empty), "");
}

/// Records, a 100 KiB line in a heredoc, a line of more than 4096 tokens, more records:
/// checkpoints by lines and by bytes, inside comments, heredocs and template sequences.
fn mixed_document() -> Vec<u8> {
    let mut text = String::from("# main.tf\r\n");
    for _ in 0..300 {
        text.push_str(RECORD);
    }
    text.push_str(&format!(
        "big = <<EOT\r\n{}\r\nresource \"c\" \"d\" {{\r\nEOT\r\n",
        "x".repeat(100 * 1024)
    ));
    text.push_str(&format!("v = [{}]\r\n", "1, ".repeat(5000)));
    for _ in 0..300 {
        text.push_str(RECORD);
    }
    ansi(&text)
}

#[test]
fn tokens_from_any_checkpoint_match_the_whole_text() {
    let mut viewer = open(TestSource::new(mixed_document()), None);
    assert_eq!(viewer.encoding(), Encoding::Windows1252);
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
    for first in [
        0,
        63,
        64,
        65,
        2701,
        2702,
        2703,
        2704,
        2705,
        2800,
        count - 20,
    ] {
        let got = viewer.tokens(first, 50).expect("tokens");
        let end = (first as usize + 50).min(whole.len());
        assert_eq!(
            got,
            whole[first as usize..end].to_vec(),
            "lines from {first}"
        );
    }
    let class = |tokens: &[Token]| tokens.iter().map(|token| token.class).collect::<Vec<_>>();
    let line = |wanted: &str| {
        text.lines()
            .position(|line| line == wanted)
            .expect("the line is there")
    };
    use TokenClass::Punctuation as P;
    assert_eq!(class(&whole[line("   a block */")]), vec![Comment]);
    assert_eq!(
        class(&whole[line("  data = <<EOT")]),
        vec![Key, Operator, Operator, Keyword]
    );
    assert_eq!(class(&whole[line("  x ${var.y}")]), vec![STRING, P, P, P]);
    assert_eq!(class(&whole[line("EOT")]), vec![Keyword]);
    assert_eq!(
        class(&whole[line("  n = 0.5 # done")]),
        vec![Key, Operator, Number, Comment]
    );
    assert_eq!(
        class(&whole[line("locals { s = \"\u{fc}\" }")]),
        vec![Keyword, P, Key, Operator, STRING, P]
    );
    // Inside the long heredoc nothing is a block.
    assert_eq!(class(&whole[line("resource \"c\" \"d\" {")]), vec![STRING]);
    let tokens = text
        .lines()
        .position(|line| line.starts_with("v = ["))
        .expect("the long line is there");
    assert_eq!(whole[tokens].len(), tokens::MAX_TOKENS_PER_LINE);
}

#[test]
fn moving_on_cancels_the_open() {
    let mut src = TestSource::new(large(4 << 20));
    src.stop_after = Some(2);
    let failure = Viewer::open(src, None).err().expect("the open stops");
    assert_eq!(failure.kind, FailureKind::Cancelled);
}

fn large(size: u64) -> Repeated {
    let body = ansi(RECORD);
    Repeated {
        head: b"# main.tf\r\n".to_vec(),
        times: size / body.len() as u64 + 1,
        body,
        tail: b"x = 1\r\n".to_vec(),
    }
}

/// `open` reads the file once in windows of at most 1 MiB and keeps its index within budget; 50
/// lines in the middle read a window or two, not the file.
fn large_document_stays_bounded(size: u64) {
    let content = large(size);
    let (size, times) = (content.size(), content.times);
    let mut viewer = open(TestSource::new(content), None);
    assert!(viewer.src.largest_read <= OPEN_CHUNK);
    assert!(viewer.src.bytes_read <= size, "one pass");
    assert!(
        viewer.index_bytes() <= INDEX_BUDGET_BYTES,
        "{}",
        viewer.index_bytes()
    );
    assert_eq!(viewer.encoding(), Encoding::Windows1252);
    assert_eq!(viewer.line_count(), times * RECORD_LINES + 2);

    let middle = u32::try_from(viewer.line_count() / 2).expect("fewer lines than u32");
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

/// Over the 16 MiB of the index budget, so the budget is held by the index's own bound, not by
/// a small file.
#[test]
fn a_17_mib_document_stays_bounded() {
    large_document_stays_bounded(17 << 20);
}

/// The acceptance case: 100 MiB. Left out of the normal run, a minute and more in a debug build;
/// `cargo test --release -- --ignored` runs it.
#[test]
#[ignore = "100 MiB through a debug build is slow; run with --ignored"]
fn a_100_mib_document_stays_bounded() {
    large_document_stays_bounded(100 << 20);
}
