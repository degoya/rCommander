use super::*;
use crate::test_support::{Content, Repeated, TestSource, VecSink, source};
use crate::types::TokenClass::{self, Function, Invalid, Literal, Number, TypeName};

/// One record of the large documents, five lines: an information line, an error with a path, and
/// the Python traceback after it - every line coloured with either `level_colour`.
const RECORD: &str = "2026-10-20 15:20:01,123 INFO  [main] app.Server - caf\u{e9} 192.168.1.10\r\n\
2026-10-20 15:20:01,124 ERROR [main] app.Server - failed: /var/data/x.db\r\n\
Traceback (most recent call last):\r\n  \
File \"/srv/app.py\", line 3, in <module>\r\n\
ValueError: bad\r\n";

fn settings(encoding: EncodingSetting, level_colour: LevelColour) -> Settings {
    Settings {
        encoding,
        level_colour,
    }
}

fn open<C: Content>(src: TestSource<C>, settings: Settings) -> Viewer<TestSource<C>> {
    Viewer::open(src, settings).expect("the document opens").0
}

fn open_auto<C: Content>(src: TestSource<C>) -> Viewer<TestSource<C>> {
    open(src, Settings::default())
}

fn text_of<C: Content>(viewer: &mut Viewer<TestSource<C>>) -> String {
    let mut sink = VecSink::default();
    assert_eq!(viewer.text(&mut sink).expect("text"), "utf-8");
    String::from_utf8(sink.0).expect("the text is UTF-8")
}

fn token(start: u32, len: u32, class: TokenClass) -> Token {
    Token { start, len, class }
}

fn utf16le(text: &str, bom: bool) -> Vec<u8> {
    let mut bytes = if bom { vec![0xFF, 0xFE] } else { Vec::new() };
    bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    bytes
}

#[test]
fn a_document_offers_highlighted_only() {
    let (viewer, document) = Viewer::open(source("INFO x\n"), Settings::default()).expect("opens");
    assert_eq!(document.outputs, vec![Output::Highlighted]);
    assert_eq!(document, viewer.document());
}

#[test]
fn probe_takes_text_and_utf16_and_refuses_binary() {
    for text in ["2026-10-20 INFO x\n", "", "\u{feff}ERROR"] {
        assert!(probe(&mut source(text), EncodingSetting::Auto), "{text}");
    }
    assert!(probe(
        &mut TestSource::new(utf16le("ERROR x\r\n", true)),
        EncodingSetting::Auto
    ));
    let bare = utf16le("ERROR x\r\n", false);
    assert!(!probe(
        &mut TestSource::new(bare.clone()),
        EncodingSetting::Auto
    ));
    assert!(probe(
        &mut TestSource::new(bare),
        EncodingSetting::Fixed(Encoding::Utf16Le)
    ));
    assert!(!probe(
        &mut TestSource::new(b"\x7fELF\x02\x01\x01\0\0".to_vec()),
        EncodingSetting::Auto
    ));
}

#[test]
fn the_text_is_utf8_after_the_byte_order_mark_and_crlf_stays_out_of_the_tokens() {
    let mut viewer = open_auto(source("\u{feff}ERROR a\r\nINFO b\r\n"));
    assert_eq!(viewer.encoding(), Encoding::Utf8);
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(text_of(&mut viewer), "ERROR a\r\nINFO b\r\n");
    assert_eq!(
        viewer.tokens(0, 2).expect("tokens"),
        vec![vec![token(0, 5, Invalid)], vec![token(0, 4, TypeName)]]
    );
}

#[test]
fn every_encoding_setting_reads_the_file_its_way() {
    let bytes = b"2026-10-20 15:20:01 ERROR caf\xe9 \x84x\x93\r\n".to_vec();
    let tokens = vec![vec![token(0, 19, Number), token(20, 5, Invalid)]];
    for (setting, encoding, text) in [
        (
            EncodingSetting::Auto,
            Encoding::Windows1252,
            "2026-10-20 15:20:01 ERROR caf\u{e9} \u{201e}x\u{201c}\r\n",
        ),
        (
            EncodingSetting::Fixed(Encoding::Windows1252),
            Encoding::Windows1252,
            "2026-10-20 15:20:01 ERROR caf\u{e9} \u{201e}x\u{201c}\r\n",
        ),
        (
            EncodingSetting::Fixed(Encoding::Utf8),
            Encoding::Utf8,
            "2026-10-20 15:20:01 ERROR caf\u{fffd} \u{fffd}x\u{fffd}\r\n",
        ),
    ] {
        let mut viewer = open(
            TestSource::new(bytes.clone()),
            settings(setting, LevelColour::Word),
        );
        assert_eq!(viewer.encoding(), encoding, "{setting:?}");
        assert_eq!(text_of(&mut viewer), text, "{setting:?}");
        assert_eq!(viewer.tokens(0, 1).expect("tokens"), tokens, "{setting:?}");
    }
}

#[test]
fn auto_checks_the_whole_file_for_utf8() {
    let mut bytes = "INFO valid \u{e4}\n".repeat(1000).into_bytes();
    bytes.extend_from_slice(b"WARN caf\xe9\n");
    let mut viewer = open_auto(TestSource::new(bytes));
    assert_eq!(viewer.encoding(), Encoding::Windows1252);
    assert!(text_of(&mut viewer).ends_with("WARN caf\u{e9}\n"));
    assert_eq!(
        viewer.tokens(1000, 1).expect("tokens"),
        vec![vec![token(0, 4, Function)]]
    );
    let mut valid = open_auto(source("INFO valid \u{e4}\n"));
    assert_eq!(valid.encoding(), Encoding::Utf8);
    assert_eq!(text_of(&mut valid), "INFO valid \u{e4}\n");
}

#[test]
fn utf16_splits_lines_by_its_own_units() {
    let text = "ERROR \u{1f600} \u{e4}\r\nINFO /tmp/\u{e4} \u{0a0d}\r\n";
    for (bytes, setting) in [
        (utf16le(text, true), EncodingSetting::Auto),
        (
            utf16le(text, false),
            EncodingSetting::Fixed(Encoding::Utf16Le),
        ),
    ] {
        let mut viewer = open(TestSource::new(bytes), settings(setting, LevelColour::Line));
        assert_eq!(viewer.encoding(), Encoding::Utf16Le);
        assert_eq!(viewer.line_count(), 2, "{setting:?}");
        assert_eq!(text_of(&mut viewer), text, "{setting:?}");
        // U+0A0D is the bytes 0D 0A in UTF-16 LE, which is no line end there.
        assert_eq!(
            viewer.tokens(0, 2).expect("tokens"),
            vec![
                vec![token(0, 13, Invalid)],
                vec![token(0, 4, TypeName), token(5, 7, Literal)],
            ],
            "{setting:?}"
        );
    }
}

#[test]
fn token_offsets_are_utf8_bytes_of_the_text() {
    let mut viewer = open(
        TestSource::new(b"\xe4\xf6\xfc ERROR /tmp/\xe4 x\r\n".to_vec()),
        settings(
            EncodingSetting::Fixed(Encoding::Windows1252),
            LevelColour::Word,
        ),
    );
    assert_eq!(
        text_of(&mut viewer),
        "\u{e4}\u{f6}\u{fc} ERROR /tmp/\u{e4} x\r\n"
    );
    assert_eq!(
        viewer.tokens(0, 1).expect("tokens"),
        vec![vec![token(7, 5, Invalid), token(13, 7, Literal)]]
    );
}

#[test]
fn only_lines_that_exist_get_tokens() {
    let mut viewer = open_auto(source("INFO a\nERROR"));
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(viewer.tokens(1, 10).expect("tokens").len(), 1);
    assert!(viewer.tokens(2, 10).expect("tokens").is_empty());
    assert!(viewer.tokens(0, 0).expect("tokens").is_empty());
    let mut empty = open_auto(source(""));
    assert_eq!(empty.line_count(), 0);
    assert!(empty.tokens(0, 1).expect("tokens").is_empty());
    assert_eq!(text_of(&mut empty), "");
}

/// Records, an error with a stack trace of 300 frames over several checkpoints, a 100 KiB line, a
/// line of more than 4096 tokens, more records.
fn mixed_document() -> Vec<u8> {
    let mut text = RECORD.repeat(100);
    text.push_str(
        "2026-10-20 15:20:03 ERROR [main] boom\r\njava.lang.IllegalStateException: x\r\n",
    );
    text.push_str(&"\tat com.example.App.run(App.java:10)\r\n".repeat(300));
    text.push_str(&format!("INFO {}\r\n", "x".repeat(100 * 1024)));
    text.push_str(&format!("{}\r\n", "[x] ".repeat(3000)));
    text.push_str(&RECORD.repeat(100));
    text.into_bytes()
}

#[test]
fn tokens_from_any_checkpoint_match_the_whole_text() {
    for colour in [LevelColour::Word, LevelColour::Line] {
        let mut viewer = open(
            TestSource::new(mixed_document()),
            settings(EncodingSetting::Auto, colour),
        );
        assert_eq!(viewer.encoding(), Encoding::Utf8);
        let text = text_of(&mut viewer);
        let count = u32::try_from(viewer.line_count()).expect("a few thousand lines");
        assert_eq!(count as usize, text.lines().count());
        let whole = viewer.tokens(0, count).expect("every line");
        assert_eq!(whole.len(), count as usize);
        for (line, tokens) in text.lines().zip(&whole) {
            let line = line.strip_suffix('\r').unwrap_or(line);
            for token in tokens {
                let start = token.start as usize;
                assert!(line.get(start..start + token.len as usize).is_some());
            }
        }
        for index in (0..count).step_by(7).chain(count - 3..count) {
            let got = viewer.tokens(index, 1).expect("tokens");
            assert_eq!(got[0], whole[index as usize], "{colour:?} line {index}");
        }
        for first in [0, 63, 64, 65, 501, 640, 700, 802, 803, count - 20] {
            let got = viewer.tokens(first, 50).expect("tokens");
            let end = (first as usize + 50).min(whole.len());
            assert_eq!(
                got,
                whole[first as usize..end].to_vec(),
                "{colour:?} lines from {first}"
            );
        }
        let frame = whole[700].clone();
        let long = text.lines().position(|line| line.starts_with("[x] [x]"));
        let long = &whole[long.expect("the line is there")];
        match colour {
            LevelColour::Word => {
                assert_eq!(frame, vec![token(1, 35, TypeName)]);
                assert_eq!(long.len(), tokens::MAX_TOKENS_PER_LINE);
            }
            // The stack trace keeps the error's colour from checkpoint to checkpoint.
            LevelColour::Line => {
                assert_eq!(frame, vec![token(0, 36, Invalid)]);
                assert_eq!(long.len(), tokens::MAX_TOKENS_PER_LINE);
            }
        }
    }
}

#[test]
fn moving_on_cancels_the_open() {
    let mut src = TestSource::new(mixed_document());
    src.stop_after = Some(1);
    let failure = Viewer::open(src, Settings::default())
        .err()
        .expect("the open stops");
    assert_eq!(failure.kind, FailureKind::Cancelled);
}

fn large(size: u64) -> Repeated {
    let body = RECORD.as_bytes().to_vec();
    Repeated {
        head: b"2026-10-20 15:20:00,000 INFO start\r\n".to_vec(),
        times: size / body.len() as u64 + 1,
        body,
        tail: b"2026-10-20 15:20:09,000 INFO stop\r\n".to_vec(),
    }
}

/// `open` reads the file once in windows of at most 1 MiB, checking it is UTF-8 and keeping the
/// stack-trace state of `line`, and keeps its index within budget; 50 lines in the middle read a
/// window or two, not the file.
fn large_document_stays_bounded(size: u64) {
    let content = large(size);
    let (size, times) = (content.size(), content.times);
    let mut viewer = open(
        TestSource::new(content),
        settings(EncodingSetting::Auto, LevelColour::Line),
    );
    assert!(viewer.src.largest_read <= OPEN_CHUNK);
    assert!(viewer.src.bytes_read <= size + HEAD_BYTES, "one pass");
    assert!(
        viewer.index_bytes() <= INDEX_BUDGET_BYTES,
        "{}",
        viewer.index_bytes()
    );
    assert_eq!(viewer.encoding(), Encoding::Utf8);
    assert_eq!(viewer.line_count(), times * 5 + 2);

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
