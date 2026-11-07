use std::time::{Duration, Instant};

use super::*;
use crate::test_support::{Content, Repeated, TestSource, VecSink, named, source, utf16le};
use crate::types::TokenClass::{
    self, Comment, Invalid, Key, Keyword, Literal, Operator, Punctuation, TypeName,
};

/// One record of the large documents, nine lines of reStructuredText in Windows-1252 (`~`
/// becomes 0xFC, a `u` umlaut), none of them blank: a title known by the line after it, a field,
/// a bullet, a directive with its content, a comment over two lines, a paragraph.
const RECORD: &str = "Gr~~e title\r\n===========\r\n:field: *value*\r\n- item ``code``\r\n.. note:: x\r\n   body *b*\r\n.. a comment\r\n   more\r\nBack **here**\r\n";
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
    let (viewer, document) = Viewer::open(source("Text.\r\n"), None).expect("opens");
    assert_eq!(document.outputs, vec![Output::Highlighted]);
    assert_eq!(document, viewer.document());
}

#[test]
fn probe_takes_text_named_for_a_format_and_refuses_the_rest() {
    for name in ["index.rst", "thesis.TEX", "my.sty", "refs.bib"] {
        assert!(probe(&mut named(name, "x\r\n")), "{name}");
    }
    assert!(probe(&mut named("empty.tex", "")));
    assert!(probe(&mut TestSource::new(ansi("% Gr~~e")).named("a.tex")));
    assert!(probe(
        &mut TestSource::new(utf16le("x\r\n", true)).named("a.rst")
    ));
    for name in ["notes.txt", "README", "a.md", ""] {
        assert!(!probe(&mut named(name, "x\r\n")), "{name}");
    }
    assert!(!probe(
        &mut TestSource::new(b"MZ\x90\0\x03\0\0\0\x04\0".to_vec()).named("a.bib")
    ));
}

#[test]
fn the_format_follows_the_extension() {
    for (name, lang) in [
        ("index.rst", Lang::Rst),
        ("a.tex", Lang::Latex),
        ("a.sty", Lang::Latex),
        ("r.bib", Lang::Bibtex),
        ("unnamed", Lang::Rst),
    ] {
        assert_eq!(open(named(name, "x"), None).lang(), lang, "{name}");
    }
    // The same text, two formats: `%` starts a comment in LaTeX only.
    let mut latex = open(named("a.tex", "% x"), None);
    let mut rst = open(named("a.rst", "% x"), None);
    assert_eq!(
        latex.tokens(0, 1).expect("tokens"),
        vec![vec![token(0, 3, Comment)]]
    );
    assert_eq!(rst.tokens(0, 1).expect("tokens"), vec![Vec::new()]);
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
fn a_title_is_known_by_the_next_line_in_both_passes() {
    let mut viewer = open(source("\u{feff}Title\r\n=====\r\nText\r\n"), None);
    assert_eq!(viewer.encoding(), Encoding::Utf8);
    assert_eq!(viewer.line_count(), 3);
    assert_eq!(text_of(&mut viewer), "Title\r\n=====\r\nText\r\n");
    let whole = viewer.tokens(0, 3).expect("tokens");
    assert_eq!(
        whole,
        vec![
            vec![token(0, 5, Keyword)],
            vec![token(0, 5, Punctuation)],
            Vec::new()
        ]
    );
    // Asked for alone, the title line still reads the line after it.
    assert_eq!(viewer.tokens(0, 1).expect("tokens"), whole[..1].to_vec());
    assert_eq!(viewer.tokens(1, 1).expect("tokens"), whole[1..2].to_vec());
}

#[test]
fn utf16_le_with_a_byte_order_mark_is_read_as_utf8_text() {
    let code = "\\emph{Gr\u{fc}\u{df}e}\r\nx $y$\r\n";
    let mut viewer = open(TestSource::new(utf16le(code, true)).named("a.tex"), None);
    assert_eq!(viewer.encoding(), Encoding::Utf16Le);
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(text_of(&mut viewer), code);
    let first = vec![
        token(0, 5, Keyword),
        token(5, 1, Punctuation),
        token(13, 1, Punctuation),
    ];
    let second = vec![
        token(2, 1, Operator),
        token(3, 1, STRING),
        token(4, 1, Operator),
    ];
    assert_eq!(
        viewer.tokens(0, 2).expect("tokens"),
        vec![first.clone(), second.clone()]
    );
    // Without a byte-order mark only the setting knows; `auto` takes the NULs for UTF-8.
    let bare = utf16le("x $y$\n", false);
    let mut viewer = open(
        TestSource::new(bare.clone()).named("a.tex"),
        Some(Encoding::Utf16Le),
    );
    assert_eq!(text_of(&mut viewer), "x $y$\n");
    assert_eq!(viewer.tokens(0, 1).expect("tokens"), vec![second]);
    assert_eq!(
        open(TestSource::new(bare).named("a.tex"), None).encoding(),
        Encoding::Utf8
    );
    // Windows-1252 when the file is not UTF-8.
    let mut viewer = open(
        TestSource::new(b"\\emph{Gr\xfc\xdfe}\r\n".to_vec()).named("a.tex"),
        None,
    );
    assert_eq!(viewer.encoding(), Encoding::Windows1252);
    assert_eq!(text_of(&mut viewer), "\\emph{Gr\u{fc}\u{df}e}\r\n");
    assert_eq!(viewer.tokens(0, 1).expect("tokens"), vec![first]);
}

#[test]
fn every_encoding_setting_reads_the_file_its_way() {
    let bytes = b"$\xe4$\r\n".to_vec();
    for (setting, text, width) in [
        (None, "$\u{e4}$\r\n", 2),
        (Some(Encoding::Windows1252), "$\u{e4}$\r\n", 2),
        (Some(Encoding::Utf8), "$\u{fffd}$\r\n", 3),
    ] {
        let mut viewer = open(TestSource::new(bytes.clone()).named("a.tex"), setting);
        assert_eq!(text_of(&mut viewer), text, "{setting:?}");
        assert_eq!(
            viewer.tokens(0, 1).expect("tokens"),
            vec![vec![
                token(0, 1, Operator),
                token(1, width, STRING),
                token(1 + width, 1, Operator)
            ]],
            "{setting:?}"
        );
    }
    // A byte-order mark is skipped only when it is the chosen encoding's.
    let utf8_bom = "\u{feff}x\n";
    let mut viewer = open(source(utf8_bom), Some(Encoding::Utf8));
    assert_eq!(text_of(&mut viewer), "x\n");
    let mut viewer = open(source(utf8_bom), Some(Encoding::Windows1252));
    assert_eq!(text_of(&mut viewer), "\u{ef}\u{bb}\u{bf}x\n");
}

#[test]
fn an_opening_left_open_shows_as_invalid() {
    // A blank line ends LaTeX math.
    let mut viewer = open(named("a.tex", "$x\r\n\r\ny\r\n"), None);
    assert_eq!(
        viewer.tokens(0, 3).expect("tokens"),
        vec![
            vec![token(0, 1, Invalid), token(1, 1, STRING)],
            Vec::new(),
            Vec::new()
        ]
    );
    // The file ends a BibTeX value.
    let mut viewer = open(named("r.bib", "@a{k, t = {open\r\nmore\r\n"), None);
    assert_eq!(
        viewer.tokens(0, 2).expect("tokens"),
        vec![
            vec![
                token(0, 2, Keyword),
                token(2, 1, Punctuation),
                token(3, 1, TypeName),
                token(4, 1, Punctuation),
                token(6, 1, Key),
                token(8, 1, Operator),
                token(10, 1, Invalid),
                token(11, 4, STRING)
            ],
            vec![token(0, 4, STRING)],
        ]
    );
    let mut viewer = open(
        named(
            "a.tex",
            "\\[ a \\]\r\n\\begin{verbatim}\r\nx\r\n\\end{verbatim}\r\n",
        ),
        None,
    );
    let tokens = viewer.tokens(0, 4).expect("tokens");
    assert!(tokens.iter().flatten().all(|token| token.class != Invalid));
}

#[test]
fn only_lines_that_exist_get_tokens() {
    let mut viewer = open(source("x\ny"), None);
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(viewer.tokens(1, 10).expect("tokens").len(), 1);
    assert!(viewer.tokens(2, 10).expect("tokens").is_empty());
    assert!(viewer.tokens(0, 0).expect("tokens").is_empty());
    let mut empty = open(source(""), None);
    assert_eq!(empty.line_count(), 0);
    assert!(empty.tokens(0, 1).expect("tokens").is_empty());
    assert_eq!(text_of(&mut empty), "");
}

/// Records, a 100 KiB line in a literal block, a line of more than 4096 tokens, more records:
/// checkpoints by lines and by bytes, inside comments, directives and literal blocks.
fn mixed_document() -> Vec<u8> {
    let mut text = String::from(".. a comment ahead\r\n");
    for _ in 0..300 {
        text.push_str(RECORD);
    }
    text.push_str(&format!(
        "Big::\r\n    {}\r\n    .. not a directive\r\n",
        "x".repeat(100 * 1024)
    ));
    text.push_str(&format!("{}\r\n", "*a* ".repeat(5000)));
    for _ in 0..300 {
        text.push_str(RECORD);
    }
    ansi(&text)
}

#[test]
fn tokens_from_any_checkpoint_match_the_whole_text() {
    let mut viewer = open(TestSource::new(mixed_document()).named("mixed.rst"), None);
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
    for first in [0, 63, 64, 65, 2701, 2702, 2703, 2704, 3100, count - 20] {
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
    assert_eq!(class(&whole[line("Gr\u{fc}\u{fc}e title")]), vec![Keyword]);
    assert_eq!(class(&whole[line("===========")]), vec![Punctuation]);
    assert_eq!(class(&whole[line(":field: *value*")]), vec![Key, Literal]);
    assert_eq!(class(&whole[line("   body *b*")]), vec![Literal]);
    assert_eq!(class(&whole[line("   more")]), vec![Comment]);
    // Inside the literal block nothing is a directive.
    assert_eq!(class(&whole[line("    .. not a directive")]), vec![STRING]);
    let tokens = text
        .lines()
        .position(|line| line.starts_with("*a* *a*"))
        .expect("the long line is there");
    assert_eq!(whole[tokens].len(), tokens::MAX_TOKENS_PER_LINE);
}

#[test]
fn moving_on_cancels_the_open() {
    let mut src = TestSource::new(large(4 << 20)).named("big.rst");
    src.stop_after = Some(2);
    let failure = Viewer::open(src, None).err().expect("the open stops");
    assert_eq!(failure.kind, FailureKind::Cancelled);
}

fn large(size: u64) -> Repeated {
    let body = ansi(RECORD);
    Repeated {
        head: b".. a comment ahead\r\n".to_vec(),
        times: size / body.len() as u64 + 1,
        body,
        tail: b"Done.\r\n".to_vec(),
    }
}

/// `open` reads the file once in windows of at most 1 MiB and keeps its index within budget; 50
/// lines in the middle read a window or two, not the file.
fn large_document_stays_bounded(size: u64) {
    let content = large(size);
    let (size, times) = (content.size(), content.times);
    let mut viewer = open(TestSource::new(content).named("big.rst"), None);
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

/// Over the 16 MiB of the index budget, so the budget is held by the index's own bound, not by
/// a small file.
#[test]
fn a_17_mib_document_stays_bounded() {
    large_document_stays_bounded(17 << 20);
}

/// 100 MiB. Left out of the normal run, a minute and more in a debug build;
/// `cargo test --release -- --ignored` runs it.
#[test]
#[ignore = "100 MiB through a debug build is slow; run with --ignored"]
fn a_100_mib_document_stays_bounded() {
    large_document_stays_bounded(100 << 20);
}

/// The time the host gives one call (`manifest.toml`, `[limits] timeout_milliseconds`).
fn time_limit() -> Duration {
    let millis = include_str!("../manifest.toml")
        .lines()
        .find_map(|line| line.strip_prefix("timeout_milliseconds = "))
        .and_then(|value| value.trim().parse::<u64>().ok())
        .expect("the manifest names its time limit");
    Duration::from_millis(millis)
}

/// The acceptance's large file: 5 MiB opens, writes its text and shows 50 lines from its middle
/// within the time the host gives one call - here even in a debug build, natively. The lexers
/// never go back, so the time grows with the file and nothing else.
#[test]
fn a_5_mib_file_stays_within_the_time_limit() {
    let limit = time_limit();
    let started = Instant::now();
    let mut viewer = open(TestSource::new(large(5 << 20)).named("big.rst"), None);
    let middle = u32::try_from(viewer.line_count() / 2).expect("fewer lines than u32");
    let tokens = viewer.tokens(middle, 50).expect("tokens");
    let mut sink = VecSink::default();
    viewer.text(&mut sink).expect("text");
    let elapsed = started.elapsed();
    assert_eq!(tokens.len(), 50);
    assert!(sink.0.len() > 5 << 20);
    assert!(elapsed < limit, "{elapsed:?} of {limit:?}");
}
