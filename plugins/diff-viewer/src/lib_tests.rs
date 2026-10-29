use super::*;
use crate::test_support::{Content, Repeated, TestSource, VecSink, named, source};
use crate::tokens::MAX_TOKENS_PER_LINE;
use crate::types::{FailureKind, TokenClass};

/// One record of the large documents: a file of a git diff with one hunk, an unchanged line with
/// non-ASCII text, a removed and two added lines and a missing final newline - eleven lines.
const RECORD: &str = concat!(
    "diff --git a/src/file.rs b/src/file.rs\n",
    "index 83db48f..bf269f4 100644\n",
    "--- a/src/file.rs\n",
    "+++ b/src/file.rs\n",
    "@@ -1,3 +1,4 @@ fn main() {\n",
    " context \u{e9}\n",
    "-removed line\n",
    "+added line\n",
    "+another\n",
    " context\n",
    "\\ No newline at end of file\n",
);

/// A `git format-patch` mail header: five lines.
const MAIL: &str = concat!(
    "From 0123456789abcdef0123456789abcdef01234567 Mon Sep 17 00:00:00 2001\n",
    "From: Jane Doe <jane@example.org>\n",
    "Subject: [PATCH] A large patch\n",
    "\n",
    "---\n",
);

/// The signature after the last hunk: two lines.
const SIGNATURE: &str = "-- \n2.43.0\n";

fn open<C: Content>(src: TestSource<C>) -> Viewer<TestSource<C>> {
    Viewer::open(src).expect("the document opens").0
}

fn text_of<C: Content>(viewer: &mut Viewer<TestSource<C>>) -> Vec<u8> {
    let mut sink = VecSink::default();
    assert_eq!(viewer.text(&mut sink).expect("text"), "utf-8");
    sink.0
}

#[test]
fn a_document_offers_highlighted_then_tree() {
    let (viewer, document) = Viewer::open(source(RECORD)).expect("opens");
    assert_eq!(document.outputs, vec![Output::Highlighted, Output::Tree]);
    assert_eq!(document, viewer.document());
}

#[test]
fn probe_takes_any_text_offered_by_its_extension() {
    for name in ["x.diff", "x.patch", "x.rej", "FIX.PATCH"] {
        assert!(probe(&mut named("just some text\n", name)), "{name}");
        assert!(probe(&mut named("", name)), "{name}");
    }
    assert!(!probe(&mut TestSource::new(b"diff \0binary".to_vec())));
}

#[test]
fn probe_takes_a_file_claimed_by_its_start_only_when_it_is_a_diff() {
    for text in [
        "diff --git a/x b/x\n",
        "diff -u a b\n",
        "\u{feff}diff -r a b\n",
        "--- a/x\n+++ b/x\n@@ -1 +1 @@\n",
        "--- a.txt\t2026-10-20 10:00:00\r\n+++ b.txt\t2026-10-20 10:01:00\r\n",
        "From 0123456789abcdef0123456789abcdef01234567 Mon Sep 17 00:00:00 2001\n",
    ] {
        assert!(probe(&mut named(text, "unnamed")), "{text:?}");
    }
    for text in [
        "From someone@example.org Mon Sep 17 00:00:00 2001\n",
        "From 0123456789abcdef Mon Sep 17\n",
        "From 0123456789abcdef0123456789abcdef0123456z Mon\n",
        "--- # a YAML document\nkey: value\n",
        "--- !tag\n+ not a diff\n",
        "--- a",
        "different\n",
    ] {
        assert!(!probe(&mut named(text, "mail.txt")), "{text:?}");
    }
}

#[test]
fn the_text_is_the_file_after_its_byte_order_mark() {
    let mut viewer = open(source("\u{feff}--- a\r\n+++ b\r\n"));
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(text_of(&mut viewer), b"--- a\r\n+++ b\r\n");
}

#[test]
fn bytes_that_are_not_utf8_are_written_as_the_replacement_character() {
    let mut viewer = open(TestSource::new(b"+\xFF\xE2\x82\n-ok\xC3".to_vec()));
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(
        String::from_utf8(text_of(&mut viewer)).expect("UTF-8"),
        "+\u{FFFD}\u{FFFD}\n-ok\u{FFFD}"
    );
}

#[test]
fn only_lines_that_exist_get_tokens() {
    let mut viewer = open(source("--- a\n+++ b"));
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(viewer.tokens(1, 10).expect("tokens").len(), 1);
    assert!(viewer.tokens(2, 10).expect("tokens").is_empty());
    assert!(viewer.tokens(0, 0).expect("tokens").is_empty());
    let mut empty = open(source(""));
    assert_eq!(empty.line_count(), 0);
    assert!(empty.tokens(0, 1).expect("tokens").is_empty());
    assert!(empty.children(None).expect("no files").is_empty());
}

/// A mail header, records, a hunk with a 100 KiB line, a combined diff, a context diff, more
/// records: checkpoints by lines and by bytes, and in every state a line can start in.
fn mixed_document() -> String {
    let mut text = String::from(MAIL);
    for _ in 0..300 {
        text.push_str(RECORD);
    }
    text.push_str("--- a/big\n+++ b/big\n@@ -1,3 +1,4 @@\n a\n");
    text.push_str(&format!("+{}\n", "y".repeat(100 * 1024)));
    text.push_str("-b\n+c\n d\n");
    text.push_str("diff --cc merged\n@@@ -1,40 -1,40 +1,40 @@@\n");
    for _ in 0..40 {
        text.push_str("  both\n");
    }
    text.push_str("*** old\n--- new\n***************\n*** 1,80 ****\n");
    for _ in 0..80 {
        text.push_str("! was\n");
    }
    text.push_str("--- 1,80 ----\n");
    for _ in 0..80 {
        text.push_str("! is\n");
    }
    for _ in 0..300 {
        text.push_str(RECORD);
    }
    text.push_str(SIGNATURE);
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
    assert!(whole.iter().all(|line| line.len() <= MAX_TOKENS_PER_LINE));
    for index in (0..count).step_by(7).chain(count - 3..count) {
        let got = viewer.tokens(index, 1).expect("tokens");
        assert_eq!(got[0], whole[index as usize], "line {index}");
    }
    for first in [0, 63, 64, 3305, 3306, 3307, 3360, 3400, 3500, count - 20] {
        let got = viewer.tokens(first, 50).expect("tokens");
        let end = (first as usize + 50).min(whole.len());
        assert_eq!(
            got,
            whole[first as usize..end].to_vec(),
            "lines from {first}"
        );
    }
    let class = |needle: &str| {
        let line = text
            .lines()
            .position(|line| line.starts_with(needle))
            .expect("the line is there");
        whole[line]
            .iter()
            .map(|token| token.class)
            .collect::<Vec<_>>()
    };
    assert_eq!(class("+yyy"), vec![TokenClass::Number]);
    assert_eq!(class("  both"), Vec::new());
    assert_eq!(class("! was"), vec![TokenClass::Invalid]);
    assert_eq!(class("! is"), vec![TokenClass::Number]);
    assert_eq!(class("2.43.0"), vec![TokenClass::Comment]);
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
        head: MAIL.as_bytes().to_vec(),
        times: size / body.len() as u64 + 1,
        body,
        tail: SIGNATURE.as_bytes().to_vec(),
    }
}

/// `open` reads the file once in windows of at most 1 MiB and keeps its index and tree within
/// budget; 50 lines in the middle read a window or two, not the file.
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
    assert_eq!(viewer.line_count(), times * 11 + 7);

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
    assert!(tokens.iter().all(|line| line.len() <= MAX_TOKENS_PER_LINE));

    let top = viewer.children(None).expect("the files");
    assert!(top.len() <= tree::PAGE + 1);
    assert_eq!(
        top.last().map(|node| node.label.as_str()),
        Some("not listed")
    );
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
