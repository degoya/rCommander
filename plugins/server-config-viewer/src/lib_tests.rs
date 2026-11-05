use super::*;
use crate::test_support::{Content, Repeated, TestSource, VecSink, named, utf16le};
use crate::types::TokenClass::{self, Function, Key, Keyword, Number, Punctuation};

/// One record of the large documents, nine lines of nginx in Windows-1252 (`~` becomes 0xFC, a
/// `u` umlaut): blocks, a statement over two lines, a string over two lines.
const RECORD: &str = "server {\r\n    listen 80;\r\n    # Gr~~e\r\n    server_name a.example\r\n        b.example;\r\n    return 200 'one\r\ntwo';\r\n    set $x 1;\r\n}\r\n";
const RECORD_LINES: u64 = 9;

fn ansi(text: &str) -> Vec<u8> {
    text.bytes()
        .map(|byte| if byte == b'~' { 0xFC } else { byte })
        .collect()
}

/// A source over `bytes` named `nginx.conf`.
fn nginx<C: Content>(content: C) -> TestSource<C> {
    let mut src = TestSource::new(content);
    src.name = "nginx.conf".to_owned();
    src
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
    let (viewer, document) =
        Viewer::open(named("Port 22\r\n", "sshd_config"), None).expect("opens");
    assert_eq!(document.outputs, vec![Output::Highlighted]);
    assert_eq!(document, viewer.document());
    assert_eq!(viewer.format(), Format::Ssh);
}

#[test]
fn probe_takes_its_names_and_server_like_conf_files() {
    for (text, name) in [
        ("user nginx;\n", "nginx.conf"),
        ("[x]\n", "nginx.conf"),
        ("RewriteEngine On\n", ".htaccess"),
        ("Port 22\n", "sshd_config"),
        ("* * * * * true\n", "crontab"),
        ("127.0.0.1 localhost\n", "hosts"),
        ("server {\n", "example.conf"),
        ("<VirtualHost *:80>\n", "site.CONF"),
        ("", "x.nginx"),
    ] {
        assert!(probe(&mut named(text, name)), "{name}: {text}");
    }
    let mut utf16 = TestSource::new(utf16le("Port 22\r\n", true));
    utf16.name = "sshd_config".to_owned();
    assert!(probe(&mut utf16));
    let mut binary = TestSource::new(b"MZ\x90\0\x03\0\0\0\x04\0".to_vec());
    binary.name = "hosts".to_owned();
    assert!(!probe(&mut binary));
}

/// RC-100-81's acceptance: an INI-like `.conf` stays with the config viewer - this plugin
/// declines it - and an nginx-like one is taken.
#[test]
fn an_ini_like_conf_is_declined_and_an_nginx_like_one_taken() {
    for text in [
        "[mysqld]\ndatadir=/var/lib/mysql\n",
        "# settings\nkey = value\n",
        "",
        "nameserver 1.1.1.1\n",
    ] {
        assert!(!probe(&mut named(text, "example.conf")), "{text}");
    }
    // Every `.conf` the config viewer declines (a block, a `;` without `=`, a tag on the first
    // line that is no comment) is taken here, so none falls through to the built-in viewer.
    for text in [
        "server {\n",
        "; x\nworker_processes auto;\n",
        "Foo bar;\n",
        "if ($a = b) {\n",
        "<VirtualHost *:80>\n",
        "</Directory>\n",
    ] {
        assert!(probe(&mut named(text, "example.conf")), "{text}");
    }
    let site = "# site\nserver {\n    listen 80;\n}\n";
    assert!(probe(&mut named(site, "example.conf")));
    let mut viewer = open(named(site, "example.conf"), None);
    assert_eq!(viewer.format(), Format::Nginx);
    assert_eq!(
        viewer.tokens(1, 1).expect("tokens")[0],
        vec![token(0, 6, Keyword), token(7, 1, Punctuation)]
    );
    let apache = open(named("LoadModule a b.so\n", "modules.conf"), None);
    assert_eq!(apache.format(), Format::Apache);
}

#[test]
fn the_text_is_utf8_after_the_byte_order_mark_and_crlf_stays_out_of_the_tokens() {
    let mut viewer = open(named("\u{feff}Port 22\r\nHost x\r\n", "ssh_config"), None);
    assert_eq!(viewer.encoding(), Encoding::Utf8);
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(text_of(&mut viewer), "Port 22\r\nHost x\r\n");
    assert_eq!(
        viewer.tokens(0, 2).expect("tokens"),
        vec![
            vec![token(0, 4, Key), token(5, 2, Number)],
            vec![token(0, 4, Keyword)],
        ]
    );
}

#[test]
fn utf16_and_windows_1252_are_read_as_utf8_text() {
    let text = "add_header X 'Gr\u{fc}\u{df}e';\r\n}\r\n";
    let mut viewer = open(nginx(utf16le(text, true)), None);
    assert_eq!(viewer.encoding(), Encoding::Utf16Le);
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(text_of(&mut viewer), text);
    let first = vec![
        token(0, 10, Key),
        token(13, 9, STRING),
        token(22, 1, Punctuation),
    ];
    assert_eq!(
        viewer.tokens(0, 2).expect("tokens"),
        vec![first.clone(), vec![token(0, 1, Punctuation)]]
    );
    let mut viewer = open(nginx(b"add_header X 'Gr\xfc\xdfe';\r\n".to_vec()), None);
    assert_eq!(viewer.encoding(), Encoding::Windows1252);
    assert_eq!(text_of(&mut viewer), "add_header X 'Gr\u{fc}\u{df}e';\r\n");
    assert_eq!(viewer.tokens(0, 1).expect("tokens"), vec![first]);
    // A byte-order mark is skipped only when it is the chosen encoding's.
    let utf8_bom = "\u{feff}x;\n";
    let mut viewer = open(named(utf8_bom, "nginx.conf"), Some(Encoding::Utf8));
    assert_eq!(text_of(&mut viewer), "x;\n");
    let mut viewer = open(named(utf8_bom, "nginx.conf"), Some(Encoding::Windows1252));
    assert_eq!(text_of(&mut viewer), "\u{ef}\u{bb}\u{bf}x;\n");
}

#[test]
fn the_setting_names_an_encoding_or_auto() {
    assert_eq!(encoding_setting(None), None);
    assert_eq!(encoding_setting(Some("auto")), None);
    assert_eq!(encoding_setting(Some("utf-16le")), Some(Encoding::Utf16Le));
    assert_eq!(
        encoding_setting(Some("windows-1252")),
        Some(Encoding::Windows1252)
    );
    assert_eq!(encoding_setting(Some("klingon")), None);
}

#[test]
fn only_lines_that_exist_get_tokens() {
    let mut viewer = open(named("x;\ny;", "nginx.conf"), None);
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(viewer.tokens(1, 10).expect("tokens").len(), 1);
    assert!(viewer.tokens(2, 10).expect("tokens").is_empty());
    assert!(viewer.tokens(0, 0).expect("tokens").is_empty());
    let mut empty = open(named("", "hosts"), None);
    assert_eq!(empty.line_count(), 0);
    assert!(empty.tokens(0, 1).expect("tokens").is_empty());
    assert_eq!(text_of(&mut empty), "");
}

/// Records, a 100 KiB line in a string, a line of more than 4096 tokens, more records:
/// checkpoints by lines and by bytes, inside statements and strings.
fn mixed_document() -> Vec<u8> {
    let mut text = String::from("# nginx\r\nhttp {\r\n");
    for _ in 0..300 {
        text.push_str(RECORD);
    }
    text.push_str(&format!(
        "return 200 '{}\r\n  listen 80;\r\n';\r\n",
        "x".repeat(100 * 1024)
    ));
    text.push_str(&format!("list {};\r\n", "1 ".repeat(5000)));
    for _ in 0..300 {
        text.push_str(RECORD);
    }
    ansi(&text)
}

#[test]
fn tokens_from_any_checkpoint_match_the_whole_text() {
    let mut viewer = open(nginx(mixed_document()), None);
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
    for first in [0, 63, 64, 65, 2702, 2703, 2704, 2705, 2800, count - 20] {
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
    assert_eq!(
        class(&whole[line("    listen 80;")]),
        vec![Key, Number, Punctuation]
    );
    // The statement goes on: no directive.
    assert_eq!(class(&whole[line("        b.example;")]), vec![Punctuation]);
    assert_eq!(class(&whole[line("two';")]), vec![STRING, Punctuation]);
    assert_eq!(
        class(&whole[line("    set $x 1;")]),
        vec![Key, Function, Number, Punctuation]
    );
    // Inside the long string nothing is a statement.
    assert_eq!(class(&whole[line("  listen 80;")]), vec![STRING]);
    let tokens = text
        .lines()
        .position(|line| line.starts_with("list "))
        .expect("the long line is there");
    assert_eq!(whole[tokens].len(), tokens::MAX_TOKENS_PER_LINE);
}

#[test]
fn moving_on_cancels_the_open() {
    let mut src = nginx(large(4 << 20));
    src.stop_after = Some(2);
    let failure = Viewer::open(src, None).err().expect("the open stops");
    assert_eq!(failure.kind, FailureKind::Cancelled);
}

fn large(size: u64) -> Repeated {
    let body = ansi(RECORD);
    Repeated {
        head: b"# nginx\r\n".to_vec(),
        times: size / body.len() as u64 + 1,
        body,
        tail: b"}\r\n".to_vec(),
    }
}

/// `open` reads the file once in windows of at most 1 MiB and keeps its index within budget; 50
/// lines in the middle read a window or two, not the file.
fn large_document_stays_bounded(size: u64) {
    let content = large(size);
    let (size, times) = (content.size(), content.times);
    let mut viewer = open(nginx(content), None);
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

/// The acceptance case: 100 MiB. Left out of the normal run, a minute and more in a debug build;
/// `cargo test --release -- --ignored` runs it.
#[test]
#[ignore = "100 MiB through a debug build is slow; run with --ignored"]
fn a_100_mib_document_stays_bounded() {
    large_document_stays_bounded(100 << 20);
}
