use super::*;
use crate::lines::LineState;
use crate::test_support::{Content, Repeated, TestSource, VecSink, named, source};
use crate::tokens::LineLexer;
use crate::types::{FailureKind, TokenClass};

fn open_with<C: Content>(src: TestSource<C>, settings: Settings) -> Viewer<TestSource<C>> {
    Viewer::open(src, settings).expect("the document opens").0
}

fn open<C: Content>(src: TestSource<C>) -> Viewer<TestSource<C>> {
    open_with(src, Settings::default())
}

fn text_of<C: Content>(viewer: &mut Viewer<TestSource<C>>) -> String {
    let mut sink = VecSink::default();
    assert_eq!(viewer.text(&mut sink).expect("text"), "utf-8");
    String::from_utf8(sink.0).expect("the text is UTF-8")
}

/// The text each token of `line` covers, with its class.
fn spans<'a>(line: &'a str, tokens: &[Token]) -> Vec<(&'a str, TokenClass)> {
    tokens
        .iter()
        .map(|token| {
            let start = token.start as usize;
            (&line[start..start + token.len as usize], token.class)
        })
        .collect()
}

fn settings(separator: &str, header: &str, encoding: &str) -> Settings {
    Settings::from_values(Some(separator), Some(header), Some(encoding))
}

#[test]
fn a_document_offers_highlighted_then_tree() {
    let (viewer, document) = Viewer::open(source("a,b\n"), Settings::default()).expect("opens");
    assert_eq!(document.outputs, vec![Output::Highlighted, Output::Tree]);
    assert_eq!(document, viewer.document());
}

#[test]
fn unknown_or_missing_settings_are_auto() {
    assert_eq!(Settings::from_values(None, None, None), Settings::default());
    assert_eq!(settings("other", "maybe", "latin-9"), Settings::default());
    let chosen = settings("pipe", "no", "windows-1252");
    assert_eq!(chosen.separator, SeparatorSetting::Fixed(b'|'));
    assert_eq!(chosen.header, HeaderSetting::No);
    assert_eq!(chosen.encoding, EncodingSetting::Windows1252);
}

#[test]
fn probe_takes_text_without_nul() {
    assert!(probe(&mut source("a,b\n1,2\n")));
    assert!(probe(&mut source("")));
    assert!(!probe(&mut source("a,\0b")));
}

#[test]
fn the_separator_setting_wins_over_the_file() {
    let text = "a;b,c\n";
    let mut comma = open_with(source(text), settings("comma", "no", "auto"));
    assert_eq!(comma.separator(), b',');
    let tokens = comma.tokens(0, 1).expect("tokens");
    assert_eq!(
        spans("a;b,c", &tokens[0]),
        vec![
            ("a;b", TokenClass::Key),
            (",", TokenClass::Punctuation),
            ("c", TokenClass::String)
        ]
    );
    let mut semicolon = open_with(source(text), settings("semicolon", "no", "auto"));
    assert_eq!(semicolon.separator(), b';');
    let tokens = semicolon.tokens(0, 1).expect("tokens");
    assert_eq!(spans("a;b,c", &tokens[0])[2], ("b,c", TokenClass::String));
    for (value, sep) in [("tab", b'\t'), ("pipe", b'|'), ("auto", b',')] {
        let viewer = open_with(source(text), settings(value, "no", "auto"));
        assert_eq!(viewer.separator(), sep, "{value}");
    }
    // `auto` takes a tab for a `.tsv`, whatever the file holds.
    assert_eq!(open(named(b"a,b\tc\n", "x.tsv")).separator(), b'\t');
    assert_eq!(open(named(b"a|b|c\n", "x.psv")).separator(), b'|');
}

#[test]
fn the_header_setting_decides_the_first_record() {
    let words = "a,b\nc,d\n";
    let numbers = "1,2\n3,4\n";
    for (text, value, header, rows) in [
        (words, "auto", true, 1),
        (words, "yes", true, 1),
        (words, "no", false, 2),
        (numbers, "auto", false, 2),
        (numbers, "yes", true, 1),
        (numbers, "no", false, 2),
    ] {
        let viewer = open_with(source(text), settings("auto", value, "auto"));
        assert_eq!(
            (viewer.has_header(), viewer.row_count()),
            (header, rows),
            "{text:?} {value}"
        );
    }
    // A header names the fields even when it is numbers.
    let mut viewer = open_with(source(numbers), settings("auto", "yes", "auto"));
    let rows = viewer.children(None).expect("rows");
    let fields = viewer.children(Some(rows[0].id)).expect("fields");
    assert_eq!(fields[1].label, "2");
}

/// Excel on a German Windows: semicolons, umlauts and the euro sign in Windows-1252.
const EXCEL: &[u8] = b"Name;Stra\xdfe;Preis\r\nM\xfcller;Hauptstr. 1;5,50 \x80\r\n";

#[test]
fn auto_reads_a_file_that_is_not_utf8_as_windows_1252() {
    let mut viewer = open(named(EXCEL, "prices.csv"));
    assert_eq!(viewer.separator(), b';');
    assert_eq!(viewer.encoding(), Encoding::Windows1252);
    assert!(viewer.has_header());
    let text = text_of(&mut viewer);
    assert_eq!(
        text,
        "Name;Stra\u{df}e;Preis\r\nM\u{fc}ller;Hauptstr. 1;5,50 \u{20ac}\r\n"
    );
    // The tokens count the bytes of the UTF-8 text.
    let tokens = viewer.tokens(1, 1).expect("tokens");
    let line = text.lines().nth(1).expect("line 2");
    assert_eq!(
        spans(line, &tokens[0]),
        vec![
            ("M\u{fc}ller", TokenClass::Key),
            (";", TokenClass::Punctuation),
            ("Hauptstr. 1", TokenClass::String),
            (";", TokenClass::Punctuation),
            ("5,50 \u{20ac}", TokenClass::Number)
        ]
    );
    let rows = viewer.children(None).expect("rows");
    let fields = viewer.children(Some(rows[0].id)).expect("fields");
    assert_eq!(fields[1].label, "Stra\u{df}e");
    assert_eq!(fields[2].value.as_deref(), Some("5,50 \u{20ac}"));
}

#[test]
fn the_encoding_setting_wins_over_the_file() {
    let mut utf8 = open_with(
        named(EXCEL, "prices.csv"),
        settings("auto", "auto", "utf-8"),
    );
    assert_eq!(utf8.encoding(), Encoding::Utf8);
    let text = text_of(&mut utf8);
    assert!(text.starts_with("Name;Stra\u{fffd}e;"), "{text}");
    let line = text.lines().nth(1).expect("line 2");
    let tokens = utf8.tokens(1, 1).expect("tokens");
    assert_eq!(
        spans(line, &tokens[0])[0],
        ("M\u{fffd}ller", TokenClass::Key)
    );

    let valid = "Gr\u{fc}n;\u{20ac}\n".as_bytes();
    assert_eq!(open(named(valid, "a.csv")).encoding(), Encoding::Utf8);
    let mut latin = open_with(
        named(valid, "a.csv"),
        settings("auto", "no", "windows-1252"),
    );
    assert_eq!(latin.encoding(), Encoding::Windows1252);
    assert_eq!(
        text_of(&mut latin),
        "Gr\u{c3}\u{bc}n;\u{e2}\u{201a}\u{ac}\n"
    );
    // A byte-order mark says UTF-8.
    let mut marked = b"\xef\xbb\xbfa;b\xff\n".to_vec();
    marked.extend_from_slice(b"c;d\n");
    assert_eq!(open(named(&marked, "a.csv")).encoding(), Encoding::Utf8);
}

#[test]
fn the_text_survives_windows_that_cut_characters() {
    // 16 bytes a line after a 5-byte head: every 1 MiB window ends inside the emoji.
    let line = "caf\u{e9};\u{20ac}\u{1f600};x\n";
    assert_eq!(line.len(), 16);
    let text = format!("head\n{}", line.repeat((3 << 20) / line.len()));
    let mut viewer = open(source(&text));
    assert_eq!(viewer.encoding(), Encoding::Utf8);
    assert_eq!(text_of(&mut viewer), text);
}

#[test]
fn only_lines_that_exist_get_tokens() {
    let mut viewer = open(source("a\nb\nc"));
    assert_eq!(viewer.line_count(), 3);
    assert_eq!(viewer.tokens(1, 10).expect("tokens").len(), 2);
    assert!(viewer.tokens(3, 10).expect("tokens").is_empty());
    assert!(viewer.tokens(0, 0).expect("tokens").is_empty());
    let mut empty = open(source(""));
    assert_eq!((empty.line_count(), empty.row_count()), (0, 0));
    assert!(empty.tokens(0, 1).expect("tokens").is_empty());
    assert!(empty.children(None).expect("rows").is_empty());
}

#[test]
fn moving_on_cancels_the_open() {
    let mut src = TestSource::new(b"a,b\n".repeat(1 << 19));
    src.stop_after = Some(1);
    let failure = Viewer::open(src, Settings::default())
        .err()
        .expect("the open stops");
    assert_eq!(failure.kind, FailureKind::Cancelled);
}

/// One record of the large documents, over two lines: every column class, a quoted field with
/// doubled quotes and the separator, non-ASCII, a line break inside quotes.
const RECORD: &str =
    "12345,\"caf\u{e9}, \"\"quoted\"\"\",-1.5,2026-10-20,true,x,y,z,9,\"two\nlines\"\n";

fn large(size: u64) -> Repeated {
    let body = RECORD.as_bytes().to_vec();
    Repeated {
        head: b"id,name,value,date,flag,a,b,c,d,note\n".to_vec(),
        times: size / body.len() as u64 + 1,
        body,
        tail: Vec::new(),
    }
}

/// `open` reads the file once in windows of at most 1 MiB and keeps its index within budget;
/// 50 lines in the middle and a page of the tree there read a few windows, not the file.
fn large_document_stays_bounded(size: u64) {
    let content = large(size);
    let (size, times) = (content.size(), content.times);
    let mut viewer = open(TestSource::new(content));
    assert!(viewer.src.largest_read <= OPEN_CHUNK);
    assert!(
        viewer.src.bytes_read <= size + 3 + u64::from(detect::HEAD_BYTES),
        "one pass"
    );
    assert!(
        viewer.index_bytes() <= INDEX_BUDGET_BYTES,
        "{}",
        viewer.index_bytes()
    );
    assert_eq!(viewer.encoding(), Encoding::Utf8);
    assert_eq!(viewer.line_count(), 1 + 2 * times);
    assert_eq!(viewer.row_count(), times);

    // The two lines of a record, lexed on their own from a record's start.
    let mut lexer = LineLexer::new(b',', LineState::default());
    let mut record_lines = RECORD.lines();
    let first = lexer.line(
        record_lines.next().unwrap_or_default().as_bytes(),
        false,
        false,
    );
    lexer.end_line();
    let second = lexer.line(
        record_lines.next().unwrap_or_default().as_bytes(),
        false,
        false,
    );
    // The quoted field keeps column 9's class, `string`, on its second line.
    assert_eq!(
        second.first().map(|token| token.class),
        Some(TokenClass::String)
    );

    let middle = (viewer.line_count() / 2) as u32;
    viewer.src.reset_counts();
    let tokens = viewer.tokens(middle, 50).expect("tokens");
    assert_eq!(tokens.len(), 50);
    assert!(
        viewer.src.bytes_read <= 1 << 20,
        "{}",
        viewer.src.bytes_read
    );
    for (index, line) in tokens.iter().enumerate() {
        let want = if (u64::from(middle) + index as u64) % 2 == 1 {
            &first
        } else {
            &second
        };
        assert_eq!(line, want, "line {}", u64::from(middle) + index as u64);
    }

    viewer.src.reset_counts();
    let mut nodes = viewer.children(None).expect("top");
    while nodes
        .first()
        .is_some_and(|node| node.label.starts_with("rows "))
    {
        let middle_range = nodes[nodes.len() / 2].id;
        nodes = viewer.children(Some(middle_range)).expect("ranges or rows");
    }
    assert_eq!(nodes.len(), 1000);
    assert!(nodes[0].label.starts_with("row "));
    let fields = viewer.children(Some(nodes[0].id)).expect("fields");
    assert_eq!(fields.len(), 10);
    assert_eq!(fields[1].label, "name");
    assert_eq!(fields[1].value.as_deref(), Some("caf\u{e9}, \"quoted\""));
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
