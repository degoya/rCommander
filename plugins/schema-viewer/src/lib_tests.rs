use super::*;
use crate::test_support::{Content, Repeated, TestSource, VecSink, named, source, utf16le};
use crate::types::TokenClass::{
    self, Comment, Function, Key, Keyword, Number, Operator, Punctuation, TypeName,
};

/// One record of the large Protobuf documents, nine lines in Windows-1252 (`~` becomes 0xFC, a
/// `u` umlaut): a block comment over two lines, a message with a map field, a service.
const RECORD: &str = "/* Gr~~e of\r\n   a message */\r\nmessage Item {\r\n  string name = 1; // n\r\n  map<string, Tag> tags = 2;\r\n}\r\nservice S {\r\n  rpc Get(Req) returns (Item);\r\n}\r\n";
const RECORD_LINES: u64 = 9;
/// One record of the GraphQL document, nine lines: a block string over three lines, a type, a
/// query.
const GRAPHQL_RECORD: &str = "\"\"\"\r\nGr~~e of a type.\r\n\"\"\"\r\ntype Item implements Node {\r\n  id: ID!\r\n  tags(first: Int = 10): [Tag] # paged\r\n}\r\nquery Q($id: ID!) { item(id: $id) { ...F } }\r\n# end\r\n";

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
    let (viewer, document) = Viewer::open(source("message M {}\r\n"), None).expect("opens");
    assert_eq!(document.outputs, vec![Output::Highlighted]);
    assert_eq!(document, viewer.document());
}

#[test]
fn probe_takes_text_named_for_a_language_and_refuses_the_rest() {
    for name in ["schema.proto", "api.graphql", "q.gql", "S.GRAPHQLS"] {
        assert!(probe(&mut named(name, "type Q {}\r\n")), "{name}");
    }
    assert!(probe(&mut named("empty.proto", "")));
    assert!(probe(
        &mut TestSource::new(ansi("// Gr~~e")).named("a.proto")
    ));
    assert!(probe(
        &mut TestSource::new(utf16le("type Q {}\r\n", true)).named("a.graphql")
    ));
    assert!(probe(
        &mut TestSource::new(utf16le("type Q {}\r\n", false)).named("a.gql")
    ));
    for name in ["notes.txt", "main.c", "README", ""] {
        assert!(!probe(&mut named(name, "type Q {}\r\n")), "{name}");
    }
    assert!(!probe(&mut TestSource::new(
        b"MZ\x90\0\x03\0\0\0\x04\0".to_vec()
    )));
}

#[test]
fn the_language_follows_the_extension() {
    for (name, lang) in [
        ("schema.proto", Lang::Proto),
        ("api.graphql", Lang::GraphQl),
        ("api.GQL", Lang::GraphQl),
        ("schema.graphqls", Lang::GraphQl),
        ("unnamed", Lang::Proto),
    ] {
        assert_eq!(open(named(name, "x"), None).lang(), lang, "{name}");
    }
    // The same text, two languages: `message` is Protobuf's keyword, a name in GraphQL.
    let mut proto = open(named("a.proto", "message M {}"), None);
    let mut graphql = open(named("a.graphql", "message M {}"), None);
    assert_eq!(
        proto.tokens(0, 1).expect("tokens")[0][0],
        token(0, 7, Keyword)
    );
    assert_eq!(
        graphql.tokens(0, 1).expect("tokens")[0][0],
        token(10, 1, Punctuation)
    );
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
    let mut viewer = open(
        source("\u{feff}syntax = \"proto3\"; // now\r\nmessage M {}\r\n"),
        None,
    );
    assert_eq!(viewer.encoding(), Encoding::Utf8);
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(
        text_of(&mut viewer),
        "syntax = \"proto3\"; // now\r\nmessage M {}\r\n"
    );
    assert_eq!(
        viewer.tokens(0, 2).expect("tokens"),
        vec![
            vec![
                token(0, 6, Keyword),
                token(7, 1, Operator),
                token(9, 8, STRING),
                token(17, 1, Punctuation),
                token(19, 6, Comment),
            ],
            vec![
                token(0, 7, Keyword),
                token(8, 1, TypeName),
                token(10, 1, Punctuation),
                token(11, 1, Punctuation),
            ],
        ]
    );
}

#[test]
fn utf16_le_and_windows_1252_are_read_as_utf8_text() {
    let code = "option s = \"Gr\u{fc}\u{df}e\";\r\nint32 x = 1;\r\n";
    let mut viewer = open(TestSource::new(utf16le(code, true)), None);
    assert_eq!(viewer.encoding(), Encoding::Utf16Le);
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(text_of(&mut viewer), code);
    let first = vec![
        token(0, 6, Keyword),
        token(7, 1, Key),
        token(9, 1, Operator),
        token(11, 9, STRING),
        token(20, 1, Punctuation),
    ];
    assert_eq!(
        viewer.tokens(0, 2).expect("tokens"),
        vec![
            first.clone(),
            vec![
                token(0, 5, TypeName),
                token(6, 1, Key),
                token(8, 1, Operator),
                token(10, 1, Number),
                token(11, 1, Punctuation),
            ],
        ]
    );
    let mut viewer = open(
        TestSource::new(b"option s = \"Gr\xfc\xdfe\";\r\n".to_vec()),
        None,
    );
    assert_eq!(viewer.encoding(), Encoding::Windows1252);
    assert_eq!(text_of(&mut viewer), "option s = \"Gr\u{fc}\u{df}e\";\r\n");
    assert_eq!(viewer.tokens(0, 1).expect("tokens"), vec![first]);
    // A byte-order mark is skipped only when it is the chosen encoding's.
    let utf8_bom = "\u{feff}message M {}\n";
    let mut viewer = open(source(utf8_bom), Some(Encoding::Utf8));
    assert_eq!(text_of(&mut viewer), "message M {}\n");
    let mut viewer = open(source(utf8_bom), Some(Encoding::Windows1252));
    assert_eq!(text_of(&mut viewer), "\u{ef}\u{bb}\u{bf}message M {}\n");
}

#[test]
fn only_lines_that_exist_get_tokens() {
    let mut viewer = open(source("message M {}\nenum E {}"), None);
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(viewer.tokens(1, 10).expect("tokens").len(), 1);
    assert!(viewer.tokens(2, 10).expect("tokens").is_empty());
    assert!(viewer.tokens(0, 0).expect("tokens").is_empty());
    let mut empty = open(source(""), None);
    assert_eq!(empty.line_count(), 0);
    assert!(empty.tokens(0, 1).expect("tokens").is_empty());
    assert_eq!(text_of(&mut empty), "");
}

/// `head`, 300 records, `long` - a 100 KiB line inside an open construct among others - a line
/// of more than 4096 tokens, 300 records: checkpoints by lines and by bytes.
fn mixed(head: &str, record: &str, long: &str, many: &str) -> Vec<u8> {
    let mut text = String::from(head);
    for _ in 0..300 {
        text.push_str(record);
    }
    text.push_str(long);
    text.push_str(many);
    for _ in 0..300 {
        text.push_str(record);
    }
    ansi(&text)
}

/// The tokens of every line of `name`'s `bytes`, asked for from many first lines, match those
/// of the whole text; returns the text and the whole text's tokens.
fn tokens_from_checkpoints(name: &str, bytes: Vec<u8>) -> (String, Vec<Vec<Token>>) {
    let mut viewer = open(TestSource::new(bytes).named(name), None);
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
    let many = text
        .lines()
        .position(|line| line.starts_with("many"))
        .expect("the long line is there");
    assert_eq!(whole[many].len(), tokens::MAX_TOKENS_PER_LINE);
    (text, whole)
}

fn classes(text: &str, whole: &[Vec<Token>], wanted: &str) -> Vec<TokenClass> {
    let line = text
        .lines()
        .position(|line| line == wanted)
        .expect("the line is there");
    whole[line].iter().map(|token| token.class).collect()
}

#[test]
fn proto_tokens_from_any_checkpoint_match_the_whole_text() {
    let long = format!(
        "/* {}\r\nmessage Inside {{\r\n*/\r\n",
        "x".repeat(100 * 1024)
    );
    let many = format!("many = [{}];\r\n", "1, ".repeat(5000));
    let bytes = mixed("syntax = \"proto3\";\r\n", RECORD, &long, &many);
    let (text, whole) = tokens_from_checkpoints("mixed.proto", bytes);
    use TokenClass::Punctuation as P;
    assert_eq!(classes(&text, &whole, "   a message */"), vec![Comment]);
    assert_eq!(
        classes(&text, &whole, "  string name = 1; // n"),
        vec![TypeName, Key, Operator, Number, P, Comment]
    );
    assert_eq!(
        classes(&text, &whole, "  map<string, Tag> tags = 2;"),
        vec![
            Keyword, P, TypeName, P, TypeName, P, Key, Operator, Number, P
        ]
    );
    assert_eq!(
        classes(&text, &whole, "  rpc Get(Req) returns (Item);"),
        vec![
            Keyword, Function, P, TypeName, P, Keyword, P, TypeName, P, P
        ]
    );
    // Inside the long comment nothing is a message.
    assert_eq!(classes(&text, &whole, "message Inside {"), vec![Comment]);
}

#[test]
fn graphql_tokens_from_any_checkpoint_match_the_whole_text() {
    let long = format!(
        "\"\"\"\r\n{}\r\ntype Inside {{\r\n\"\"\"\r\n",
        "x".repeat(100 * 1024)
    );
    let many = format!("many(a: [{}])\r\n", "1, ".repeat(5000));
    let bytes = mixed("schema { query: Q }\r\n", GRAPHQL_RECORD, &long, &many);
    let (text, whole) = tokens_from_checkpoints("mixed.graphql", bytes);
    use TokenClass::Punctuation as P;
    assert_eq!(
        classes(&text, &whole, "Gr\u{fc}\u{fc}e of a type."),
        vec![STRING]
    );
    assert_eq!(
        classes(&text, &whole, "type Item implements Node {"),
        vec![Keyword, TypeName, Keyword, TypeName, P]
    );
    assert_eq!(
        classes(&text, &whole, "  id: ID!"),
        vec![Key, P, TypeName, Operator]
    );
    assert_eq!(
        classes(
            &text,
            &whole,
            "query Q($id: ID!) { item(id: $id) { ...F } }"
        ),
        vec![
            Keyword, Function, P, Key, P, TypeName, Operator, P, P, Function, P, Key, P, Key, P, P,
            Operator, Function, P, P
        ]
    );
    // Inside the long block string nothing is a type.
    assert_eq!(classes(&text, &whole, "type Inside {"), vec![STRING]);
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
        head: b"syntax = \"proto3\";\r\n".to_vec(),
        times: size / body.len() as u64 + 1,
        body,
        tail: b"message End {}\r\n".to_vec(),
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
