use super::*;
use crate::test_support::{Content, Repeated, TestSource, VecSink, source, utf16le};
use crate::types::TokenClass::{
    self, Comment, Function, Invalid, Key, Keyword, Number, Operator, Punctuation,
};

/// One record of the large documents, nine lines in Windows-1252 (`~` becomes 0xFC, a `u`
/// umlaut): a function with an attribute, a here-string with a variable and `$( )` in it, a block
/// comment over two lines, a backtick continuation.
const RECORD: &str = "function Get-Item {\r\n    param([string]$Path = \"C:\\temp\")\r\n    $h = @\"\r\n    Gr~~e $Path $(Get-Date)\r\n\"@\r\n    <# block\r\n    comment #> Write-Output $h -f 1kb | Out-Null `\r\n        -Force\r\n}\r\n";
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
    let (viewer, document) = Viewer::open(source("Get-Date\r\n"), None).expect("opens");
    assert_eq!(document.outputs, vec![Output::Highlighted]);
    assert_eq!(document, viewer.document());
}

#[test]
fn probe_takes_text_and_utf16_and_refuses_binary() {
    for text in ["Get-Date\r\n", "", "\u{feff}$x = 1", "plain"] {
        assert!(probe(&mut source(text)), "{text}");
    }
    assert!(probe(&mut TestSource::new(ansi("# Gr~~e"))));
    assert!(probe(&mut TestSource::new(utf16le("$x = 1\r\n", true))));
    assert!(probe(&mut TestSource::new(utf16le("$x = 1\r\n", false))));
    assert!(!probe(&mut TestSource::new(
        b"MZ\x90\0\x03\0\0\0\x04\0".to_vec()
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
    let mut viewer = open(source("\u{feff}Get-Date # now\r\n$x = 1\r\n"), None);
    assert_eq!(viewer.encoding(), Encoding::Utf8);
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(text_of(&mut viewer), "Get-Date # now\r\n$x = 1\r\n");
    assert_eq!(
        viewer.tokens(0, 2).expect("tokens"),
        vec![
            vec![token(0, 8, Function), token(9, 5, Comment)],
            vec![token(0, 2, Key), token(3, 1, Operator), token(5, 1, Number)],
        ]
    );
}

#[test]
fn utf16_le_with_a_byte_order_mark_is_read_as_utf8_text() {
    let script = "Write-Host 'Gr\u{fc}\u{df}e'\r\n$x = 1\r\n";
    let mut viewer = open(TestSource::new(utf16le(script, true)), None);
    assert_eq!(viewer.encoding(), Encoding::Utf16Le);
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(text_of(&mut viewer), script);
    assert_eq!(
        viewer.tokens(0, 2).expect("tokens"),
        vec![
            vec![token(0, 10, Function), token(11, 9, STRING)],
            vec![token(0, 2, Key), token(3, 1, Operator), token(5, 1, Number)],
        ]
    );
    // Without a byte-order mark only the setting knows; `auto` takes the NULs for UTF-8.
    let bare = utf16le("$a = 'x'\n", false);
    let mut viewer = open(TestSource::new(bare.clone()), Some(Encoding::Utf16Le));
    assert_eq!(text_of(&mut viewer), "$a = 'x'\n");
    assert_eq!(
        viewer.tokens(0, 1).expect("tokens")[0],
        vec![token(0, 2, Key), token(3, 1, Operator), token(5, 3, STRING)]
    );
    assert_eq!(open(TestSource::new(bare), None).encoding(), Encoding::Utf8);
}

#[test]
fn windows_1252_when_the_file_is_not_utf8() {
    let mut viewer = open(
        TestSource::new(b"Write-Host 'Gr\xfc\xdfe'\r\n".to_vec()),
        None,
    );
    assert_eq!(viewer.encoding(), Encoding::Windows1252);
    assert_eq!(text_of(&mut viewer), "Write-Host 'Gr\u{fc}\u{df}e'\r\n");
    assert_eq!(
        viewer.tokens(0, 1).expect("tokens"),
        vec![vec![token(0, 10, Function), token(11, 9, STRING)]]
    );
}

#[test]
fn every_encoding_setting_reads_the_file_its_way() {
    let bytes = b"$s = '\xe4'\r\n".to_vec();
    for (setting, text, width) in [
        (None, "$s = '\u{e4}'\r\n", 4),
        (Some(Encoding::Windows1252), "$s = '\u{e4}'\r\n", 4),
        (Some(Encoding::Utf8), "$s = '\u{fffd}'\r\n", 5),
    ] {
        let mut viewer = open(TestSource::new(bytes.clone()), setting);
        assert_eq!(text_of(&mut viewer), text, "{setting:?}");
        assert_eq!(
            viewer.tokens(0, 1).expect("tokens"),
            vec![vec![
                token(0, 2, Key),
                token(3, 1, Operator),
                token(5, width, STRING)
            ]],
            "{setting:?}"
        );
    }
    // `utf-16le` reads the same ten bytes as five UTF-16 units, none of them a line break.
    let viewer = open(TestSource::new(bytes), Some(Encoding::Utf16Le));
    assert_eq!(viewer.encoding(), Encoding::Utf16Le);
    assert_eq!(viewer.line_count(), 1);
    // A byte-order mark is skipped only when it is the chosen encoding's.
    let utf8_bom = "\u{feff}exit\n";
    let mut viewer = open(source(utf8_bom), Some(Encoding::Utf8));
    assert_eq!(text_of(&mut viewer), "exit\n");
    let mut viewer = open(source(utf8_bom), Some(Encoding::Windows1252));
    assert_eq!(text_of(&mut viewer), "\u{ef}\u{bb}\u{bf}exit\n");
    let mut viewer = open(TestSource::new(utf16le("exit", true)), Some(Encoding::Utf8));
    assert!(text_of(&mut viewer).starts_with("\u{fffd}\u{fffd}e\0x\0"));
}

#[test]
fn a_string_the_file_leaves_open_shows_its_opening_as_invalid() {
    let mut viewer = open(source("Write-Host \"open\r\nstill open\r\n"), None);
    assert_eq!(
        viewer.tokens(0, 2).expect("tokens"),
        vec![
            vec![
                token(0, 10, Function),
                token(11, 1, Invalid),
                token(12, 4, STRING)
            ],
            vec![token(0, 10, STRING)],
        ]
    );
    let mut viewer = open(source("$h = @'\r\nbody\r\n"), None);
    assert_eq!(
        viewer.tokens(0, 2).expect("tokens"),
        vec![
            vec![
                token(0, 2, Key),
                token(3, 1, Operator),
                token(5, 2, Invalid)
            ],
            vec![token(0, 4, STRING)],
        ]
    );
    let mut viewer = open(source("'a' + \"b\"\r\n@\"\r\nx\r\n\"@\r\n"), None);
    let tokens = viewer.tokens(0, 4).expect("tokens");
    assert!(tokens.iter().flatten().all(|token| token.class != Invalid));
}

#[test]
fn only_lines_that_exist_get_tokens() {
    let mut viewer = open(source("exit\nGet-Date"), None);
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(viewer.tokens(1, 10).expect("tokens").len(), 1);
    assert!(viewer.tokens(2, 10).expect("tokens").is_empty());
    assert!(viewer.tokens(0, 0).expect("tokens").is_empty());
    let mut empty = open(source(""), None);
    assert_eq!(empty.line_count(), 0);
    assert!(empty.tokens(0, 1).expect("tokens").is_empty());
    assert_eq!(text_of(&mut empty), "");
}

/// Records, a 100 KiB line continued by a backtick, a line of more than 4096 variables, more
/// records: checkpoints by lines and by bytes, inside here-strings, comments and continuations.
fn mixed_document() -> Vec<u8> {
    let mut text = String::from("#Requires -Version 5.1\r\nSet-StrictMode -Version Latest\r\n");
    for _ in 0..300 {
        text.push_str(RECORD);
    }
    text.push_str(&format!(
        "Write-Output \"{}\" `\r\n",
        "x".repeat(100 * 1024)
    ));
    text.push_str("  Get-Date\r\n");
    text.push_str(&format!("Write-Output{}\r\n", " $a".repeat(5000)));
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
    for first in [0, 63, 64, 65, 2701, 2702, 2703, 2800, count - 20] {
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
    // A backtick continues the command: the next line's `Get-Date` is an argument.
    assert!(whole[line("  Get-Date")].is_empty());
    assert_eq!(class(&whole[line("        -Force")]), vec![Operator]);
    assert_eq!(
        class(&whole[line("    comment #> Write-Output $h -f 1kb | Out-Null `")]),
        vec![Comment, Function, Key, Operator, Number, Operator, Function]
    );
    assert_eq!(
        class(&whole[line("    Gr\u{fc}\u{fc}e $Path $(Get-Date)")]),
        vec![STRING, Key, STRING, Punctuation, Function, Punctuation]
    );
    assert_eq!(class(&whole[line("function Get-Item {")])[0], Keyword);
    let variables = text
        .lines()
        .position(|line| line.ends_with("$a $a"))
        .expect("the long line is there");
    assert_eq!(whole[variables].len(), tokens::MAX_TOKENS_PER_LINE);
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
        head: b"#Requires -Version 5.1\r\n".to_vec(),
        times: size / body.len() as u64 + 1,
        body,
        tail: b"exit 0\r\n".to_vec(),
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
