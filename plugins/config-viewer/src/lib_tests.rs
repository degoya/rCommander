use super::*;
use crate::test_support::{Content, Repeated, TestSource, VecSink, named};
use crate::types::TokenClass::{self, Invalid, Key, Keyword, Punctuation, String as Str};
use crate::types::{FailureKind, encoding_setting};

/// One record of the large documents, six lines in Windows-1252: a section, a value with umlauts,
/// a number, a comment, a literal and a blank line.
const RECORD: &[u8] =
    b"[section]\r\nname = Gr\xf6\xdfe\r\nport = 8080\r\n; comment\r\nenabled = true\r\n\r\n";
/// One record of the TOML documents, eleven lines: an array of tables, a multi-line string and an
/// array over lines.
const TOML_RECORD: &str = "[[item]]\nid = 1\nnote = \"\"\"\nline one\nline two\n\"\"\"\ntags = [\n  \"a\",\n  \"b\",\n]\n# c\n";

fn open<C: Content>(src: TestSource<C>, setting: Option<Encoding>) -> Viewer<TestSource<C>> {
    Viewer::open(src, setting).expect("the document opens").0
}

fn text_of<C: Content>(viewer: &mut Viewer<TestSource<C>>) -> String {
    let mut sink = VecSink::default();
    assert_eq!(viewer.text(&mut sink).expect("text"), "utf-8");
    String::from_utf8(sink.0).expect("the text is UTF-8")
}

fn token(start: u32, len: u32, class: TokenClass) -> Token {
    Token { start, len, class }
}

fn utf16(text: &str, little: bool) -> Vec<u8> {
    let mut bytes = if little {
        vec![0xFF, 0xFE]
    } else {
        vec![0xFE, 0xFF]
    };
    for unit in text.encode_utf16() {
        bytes.extend(if little {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        });
    }
    bytes
}

#[test]
fn a_document_offers_highlighted_then_tree() {
    let (viewer, document) = Viewer::open(named(b"[a]\n", "a.ini"), None).expect("opens");
    assert_eq!(document.outputs, vec![Output::Highlighted, Output::Tree]);
    assert_eq!(document, viewer.document());
}

#[test]
fn probe_takes_text_utf16_and_of_lock_files_cargo_lock_only() {
    assert!(probe(&mut named(b"[a]\nk=1\n", "a.ini")));
    assert!(probe(&mut named(b"", "empty.ini")));
    assert!(probe(&mut named(&utf16("[a]", true), "x.reg")));
    assert!(probe(&mut named(&utf16("[a]", false), "x.reg")));
    assert!(!probe(&mut named(b"MZ\0\x01", "x.cfg")));
    assert!(!probe(&mut named(b"# yarn", "yarn.lock")));
    assert!(probe(&mut named(b"[[package]]\n", "Cargo.lock")));
}

#[test]
fn the_format_comes_from_the_name() {
    for (name, format) in [
        ("desktop.ini", Format::Ini),
        ("setup.INF", Format::Ini),
        (".editorconfig", Format::Ini),
        (".gitconfig", Format::Ini),
        ("nginx.conf", Format::Ini),
        ("", Format::Ini),
        ("export.reg", Format::Registry),
        (".env", Format::Env),
        ("prod.env", Format::Env),
        (".env.local", Format::Env),
        ("app.properties", Format::Properties),
        ("Cargo.toml", Format::Toml),
        ("Cargo.lock", Format::Toml),
        (".gitignore", Format::Lines),
        (".gitattributes", Format::Lines),
    ] {
        assert_eq!(Format::from_name(name), format, "{name}");
    }
    let viewer = open(named(b"a = 1\n", "Cargo.toml"), None);
    assert_eq!(viewer.format(), Format::Toml);
}

#[test]
fn the_setting_names_an_encoding_or_auto() {
    assert_eq!(encoding_setting(Some("utf-16le")), Some(Encoding::Utf16Le));
    assert_eq!(
        encoding_setting(Some("windows-1252")),
        Some(Encoding::Windows1252)
    );
    assert_eq!(encoding_setting(Some("utf-8")), Some(Encoding::Utf8));
    assert_eq!(encoding_setting(Some("auto")), None);
    assert_eq!(encoding_setting(Some("latin-9")), None);
    assert_eq!(encoding_setting(None), None);
}

#[test]
fn auto_takes_a_byte_order_mark_then_utf8_then_windows_1252() {
    let mut viewer = open(named("\u{feff}k=\u{e4}\n".as_bytes(), "a.ini"), None);
    assert_eq!(viewer.encoding(), Encoding::Utf8);
    assert_eq!(text_of(&mut viewer), "k=\u{e4}\n");

    let ansi = b"[Gr\xf6\xdfe]\r\nName=J\xfcrgen\r\n";
    let mut viewer = open(named(ansi, "desktop.ini"), None);
    assert_eq!(viewer.encoding(), Encoding::Windows1252);
    assert_eq!(
        text_of(&mut viewer),
        "[Gr\u{f6}\u{df}e]\r\nName=J\u{fc}rgen\r\n"
    );
    // Offsets are UTF-8 bytes of that text: the umlauts take two each.
    assert_eq!(
        viewer.tokens(0, 2).expect("tokens"),
        vec![
            vec![token(0, 9, Keyword)],
            vec![token(0, 4, Key), token(4, 1, Punctuation), token(5, 7, Str)]
        ]
    );

    let mut viewer = open(named(&utf16("a = 1\n", false), "x.ini"), None);
    assert_eq!(viewer.encoding(), Encoding::Utf16Be);
    assert_eq!(text_of(&mut viewer), "a = 1\n");

    let viewer = open(named("k=\u{e4}\n".as_bytes(), "a.ini"), None);
    assert_eq!(viewer.encoding(), Encoding::Utf8);
}

#[test]
fn the_setting_overrides_what_auto_would_take() {
    let ansi = b"Name=J\xfcrgen\n";
    let mut viewer = open(named(ansi, "a.ini"), Some(Encoding::Utf8));
    assert_eq!(text_of(&mut viewer), "Name=J\u{fffd}rgen\n");
    // A byte-order mark read in a code page is three characters of it.
    let mut viewer = open(
        named("\u{feff}k=1\n".as_bytes(), "a.ini"),
        Some(Encoding::Windows1252),
    );
    assert_eq!(viewer.encoding(), Encoding::Windows1252);
    assert_eq!(text_of(&mut viewer), "\u{ef}\u{bb}\u{bf}k=1\n");
    let mut viewer = open(
        named(&utf16("k=1\n", true), "a.reg"),
        Some(Encoding::Utf16Le),
    );
    assert_eq!(text_of(&mut viewer), "k=1\n");
}

#[test]
fn a_registry_export_in_utf16_is_coloured_on_its_text() {
    let text = "Windows Registry Editor Version 5.00\r\n\r\n[HKEY_CURRENT_USER\\Software\\\u{e4}]\r\n\"N\u{e4}me\"=dword:00000001\r\n";
    let mut viewer = open(named(&utf16(text, true), "export.reg"), None);
    assert_eq!(viewer.encoding(), Encoding::Utf16Le);
    assert_eq!(viewer.line_count(), 4);
    assert_eq!(text_of(&mut viewer), text);
    let tokens = viewer.tokens(2, 2).expect("tokens");
    assert_eq!(
        tokens,
        vec![
            vec![token(0, 31, Keyword)],
            vec![
                token(0, 7, Key),
                token(7, 1, Punctuation),
                token(8, 6, TokenClass::TypeName),
                token(14, 8, TokenClass::Number)
            ]
        ]
    );
}

#[test]
fn crlf_stays_in_the_text_and_out_of_the_tokens() {
    let mut viewer = open(named(b"[a]\r\nk = v\r\n", "a.ini"), None);
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(text_of(&mut viewer), "[a]\r\nk = v\r\n");
    assert_eq!(
        viewer.tokens(0, 5).expect("tokens"),
        vec![
            vec![token(0, 3, Keyword)],
            vec![token(0, 1, Key), token(2, 1, Punctuation), token(4, 1, Str)]
        ]
    );
}

#[test]
fn only_lines_that_exist_get_tokens() {
    let mut viewer = open(named(b"a=1\nb", "a.ini"), None);
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(viewer.tokens(1, 10).expect("tokens").len(), 1);
    assert!(viewer.tokens(2, 10).expect("tokens").is_empty());
    assert!(viewer.tokens(0, 0).expect("tokens").is_empty());
    let mut empty = open(named(b"", "a.ini"), None);
    assert_eq!(empty.line_count(), 0);
    assert!(empty.tokens(0, 1).expect("tokens").is_empty());
    assert_eq!(text_of(&mut empty), "");
    assert!(empty.children(None).expect("children").is_empty());
}

/// Records, a 100 KiB line, a line of more than 4096 tokens, more records: checkpoints by lines
/// and by bytes, inside multi-line strings and arrays.
fn mixed_document() -> String {
    let mut text = String::from("title = \"x\"\n");
    for _ in 0..300 {
        text.push_str(TOML_RECORD);
    }
    text.push_str(&format!("long = \"{}\"\n", "x".repeat(100 * 1024)));
    text.push_str(&format!("many = [{}]\n", "1,".repeat(5000)));
    for _ in 0..300 {
        text.push_str(TOML_RECORD);
    }
    text
}

#[test]
fn tokens_from_any_checkpoint_match_the_whole_text() {
    let mut viewer = open(named(mixed_document().as_bytes(), "big.toml"), None);
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
    for first in [0, 63, 64, 65, 3301, 3302, 3303, 3400, count - 20] {
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
            .position(|line| line == needle)
            .expect("the line is there")
    };
    // Inside the multi-line string and the array: a string, never a key or a header.
    assert_eq!(whole[line("line one")], vec![token(0, 8, Str)]);
    assert_eq!(
        whole[line("  \"a\",")],
        vec![token(2, 3, Str), token(5, 1, Punctuation)]
    );
    let many = text
        .lines()
        .position(|line| line.starts_with("many"))
        .expect("many");
    assert_eq!(whole[many].len(), tokens::MAX_TOKENS_PER_LINE);
    // The long line is lexed up to the cap: its string never closes there.
    let long = text
        .lines()
        .position(|line| line.starts_with("long"))
        .expect("long");
    assert_eq!(whole[long][2].class, Invalid);
    assert_eq!(whole[long + 1][0], token(0, 4, Key));
}

#[test]
fn moving_on_cancels_the_open() {
    let mut src = named(mixed_document().as_bytes(), "big.toml");
    src.stop_after = Some(1);
    let failure = Viewer::open(src, None).err().expect("the open stops");
    assert_eq!(failure.kind, FailureKind::Cancelled);
}

fn large(size: u64) -> TestSource<Repeated> {
    let mut src = TestSource::new(Repeated {
        head: b"; generated\r\n".to_vec(),
        times: size / RECORD.len() as u64 + 1,
        body: RECORD.to_vec(),
        tail: b"[last]\r\nend = 1\r\n".to_vec(),
    });
    src.name = "big.ini".to_owned();
    src
}

/// `open` reads the file once in windows of at most 1 MiB and keeps its index within budget; 50
/// lines in the middle and a section's keys read a window or two, not the file.
fn large_document_stays_bounded(size: u64) {
    let src = large(size);
    let (size, times) = (src.content.size(), src.content.times);
    let mut viewer = open(src, None);
    assert!(viewer.src.largest_read <= OPEN_CHUNK);
    assert_eq!(viewer.src.bytes_read, size, "one pass");
    assert!(
        viewer.index_bytes() <= INDEX_BUDGET_BYTES,
        "{}",
        viewer.index_bytes()
    );
    assert_eq!(viewer.encoding(), Encoding::Windows1252);
    assert_eq!(viewer.line_count(), times * 6 + 3);

    let middle = u32::try_from(viewer.line_count() / 2).expect("fewer lines than u32");
    viewer.src.reset_counts();
    let tokens = viewer.tokens(middle, 50).expect("tokens");
    assert_eq!(tokens.len(), 50);
    assert!(
        viewer.src.bytes_read <= 1 << 20,
        "{}",
        viewer.src.bytes_read
    );
    assert!(tokens.iter().filter(|line| !line.is_empty()).count() >= 40);

    viewer.src.reset_counts();
    let top = viewer.children(None).expect("top");
    assert_eq!(
        top.len(),
        17,
        "16384 sections and the rest named, in ranges"
    );
    let sections = viewer.children(Some(top[8].id)).expect("a range");
    assert_eq!(sections.len(), 1000);
    let keys = viewer.children(Some(sections[500].id)).expect("keys");
    let shown: Vec<(&str, Option<&str>)> = keys
        .iter()
        .map(|node| (node.label.as_str(), node.value.as_deref()))
        .collect();
    assert_eq!(
        shown,
        [
            ("name", Some("Gr\u{f6}\u{df}e")),
            ("port", Some("8080")),
            ("enabled", Some("true"))
        ]
    );
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
