use super::*;
use crate::test_support::{Content, Repeated, TestSource, VecSink, source};
use crate::types::{FailureKind, TokenClass};

/// One record of the large documents: a function, a local assignment, a case, a substitution in
/// a string, a heredoc with an expansion and non-ASCII text, comments - ten lines.
const RECORD: &str = concat!(
    "deploy() {\n",
    "  local name=\"app $1\" # the app\n",
    "  case \"$name\" in\n",
    "    a*) echo \"$(date +%s)\" >&2 ;;\n",
    "    *) printf '%s\\n' \"${name:-none}\" ;;\n",
    "  esac\n",
    "  cat <<EOF\n",
    "value: $name \u{e9}\n",
    "EOF\n",
    "}\n",
);

fn open<C: Content>(src: TestSource<C>) -> Viewer<TestSource<C>> {
    Viewer::open(src).expect("the document opens").0
}

fn text_of<C: Content>(viewer: &mut Viewer<TestSource<C>>) -> Vec<u8> {
    let mut sink = VecSink::default();
    assert_eq!(viewer.text(&mut sink).expect("text"), "utf-8");
    sink.0
}

#[test]
fn a_document_offers_highlighted_only() {
    let (viewer, document) = Viewer::open(source("echo hi\n")).expect("opens");
    assert_eq!(document.outputs, vec![Output::Highlighted]);
    assert_eq!(document, viewer.document());
}

#[test]
fn probe_takes_shell_scripts() {
    for text in [
        "#!/bin/sh\n",
        "#!/bin/bash -e\n",
        "#!/usr/bin/env bash\n",
        "#! /bin/zsh\n",
        "#!/usr/bin/env -S bash -x\n",
        "#!/usr/local/bin/ksh93\n",
        "#!/bin/dash\r\n",
        "#!/usr/bin/env VAR=1 sh\n",
        "\u{feff}#!/bin/sh\n",
    ] {
        assert!(probe(&mut source(text)), "{text}");
    }
}

#[test]
fn probe_takes_a_file_without_a_shebang() {
    // Offered by its extension (`.sh`, `.bashrc`, ...): any text is taken.
    for text in ["echo hi\n", "", "# just a comment\n", "x=1"] {
        assert!(probe(&mut source(text)), "{text:?}");
    }
}

#[test]
fn probe_declines_other_interpreters_and_binary_files() {
    for text in [
        "#!/usr/bin/env python3\n",
        "#!/usr/bin/perl\n",
        "#!/bin/shx\n",
        "#!\n",
        "#!/usr/bin/env\n",
        "#!/usr/bin/env -S\n",
    ] {
        assert!(!probe(&mut source(text)), "{text:?}");
    }
    assert!(!probe(&mut TestSource::new(b"#!/bin/sh\n\0\x01".to_vec())));
    assert!(!probe(&mut TestSource::new(
        b"\x7fELF\x02\x01\x01\0".to_vec()
    )));
}

#[test]
fn the_text_is_the_file_after_its_byte_order_mark() {
    let mut viewer = open(source("\u{feff}echo 1\r\nexit\r\n"));
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(text_of(&mut viewer), b"echo 1\r\nexit\r\n");
}

#[test]
fn bytes_that_are_not_utf8_are_written_as_the_replacement_character() {
    let mut viewer = open(TestSource::new(b"echo \xFF\xE2\x82\nok\xC3".to_vec()));
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(
        String::from_utf8(text_of(&mut viewer)).expect("UTF-8"),
        "echo \u{FFFD}\u{FFFD}\nok\u{FFFD}"
    );
}

#[test]
fn only_lines_that_exist_get_tokens() {
    let mut viewer = open(source("echo 1\nexit 2"));
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(viewer.tokens(1, 10).expect("tokens").len(), 1);
    assert!(viewer.tokens(2, 10).expect("tokens").is_empty());
    assert!(viewer.tokens(0, 0).expect("tokens").is_empty());
    let mut empty = open(source(""));
    assert_eq!(empty.line_count(), 0);
    assert!(empty.tokens(0, 1).expect("tokens").is_empty());
}

/// Records, a 100 KiB line, a literal heredoc, a string over many lines, a substitution over
/// many lines with continued commands, more records: checkpoints by lines and by bytes, and in
/// every state a line can start in.
fn mixed_document() -> String {
    let mut text = String::from("#!/bin/bash\n");
    for index in 0..300 {
        text.push_str(&RECORD.replace("deploy", &format!("deploy_{index}")));
    }
    text.push_str(&format!("echo \"{}\"\n", "y".repeat(100 * 1024)));
    text.push_str("cat <<'EOF'\n");
    for index in 0..200 {
        text.push_str(&format!("  literal {index} $x \"open ' # not a comment\n"));
    }
    text.push_str("EOF\nmsg=\"first\n");
    for _ in 0..200 {
        text.push_str("  still quoted $v ; done # (\n");
    }
    text.push_str("  last\"\nx=$(\n");
    for _ in 0..200 {
        text.push_str("  echo nested \\\n    continued $((1 + 2))\n");
    }
    text.push_str(")\n");
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
    for first in [0, 63, 64, 3001, 3002, 3100, 3300, 3500, 3700, count - 20] {
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
    use crate::types::TokenClass::{Key, Number, Operator, Punctuation};
    assert_eq!(
        class(&whole[line("literal 150 ")]),
        vec![TokenClass::String]
    );
    assert_eq!(
        class(&whole[line("still quoted")]),
        vec![TokenClass::String, Key, TokenClass::String]
    );
    assert_eq!(
        class(&whole[line("continued")]),
        vec![Punctuation, Number, Operator, Number, Punctuation]
    );
    assert_eq!(
        class(&whole[line("value: ")]),
        vec![TokenClass::String, Key, TokenClass::String]
    );
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
        head: b"#!/bin/bash\nset -e\n".to_vec(),
        times: size / body.len() as u64 + 1,
        body,
        tail: b"exit 0\n".to_vec(),
    }
}

/// `open` reads the file once in windows of at most 1 MiB and keeps its index within budget;
/// 50 lines in the middle read a window or two, not the file.
fn large_document_stays_bounded(size: u64) {
    let content = large(size);
    let (size, times) = (content.size(), content.times);
    let mut viewer = open(TestSource::new(content));
    assert!(viewer.src.largest_read <= OPEN_CHUNK);
    assert!(viewer.src.bytes_read <= size + 3, "one pass");
    assert!(
        viewer.index_bytes() <= INDEX_BUDGET_BYTES,
        "{}",
        viewer.index_bytes()
    );
    assert_eq!(viewer.line_count(), times * 10 + 3);
    assert!(viewer.unclosed.is_empty());

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
