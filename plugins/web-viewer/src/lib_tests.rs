use super::*;
use crate::test_support::{Content, Repeated, TestSource, VecSink, highlighted, named};
use crate::types::{FailureKind, TokenClass};

/// One record of the large documents, ten lines of a PHP page: HTML with PHP in its text and
/// attributes, a `style` and an `onclick` attribute, a script with template strings nested in
/// each other, a regular expression and a division, a style element.
const RECORD: &str = concat!(
    "<div class=\"row\" onclick=\"toggle(this)\">\n",
    "  <?php foreach ($items as $item): ?>\n",
    "  <span style=\"color: #c00\">&lt;<?= $item ?>&gt; \u{e9}</span>\n",
    "  <?php endforeach; ?>\n",
    "</div>\n",
    "<script>\n",
    "const tpl = `item ${items.map(i => `<li>${i}</li>`).join(\"\")}`;\n",
    "if (a / 2 > 1) x = /re[/]/g.test(s);\n",
    "</script>\n",
    "<style>.row > span:hover { margin: 0 auto; }</style>\n",
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
    let (viewer, document) = Viewer::open(named(b"<p>hi</p>\n", "a.html")).expect("opens");
    assert_eq!(document.outputs, vec![Output::Highlighted]);
    assert_eq!(document, viewer.document());
}

#[test]
fn probe_takes_text_and_declines_binary_files() {
    for text in ["<p>x</p>", "", "a { }", "\u{feff}<?php"] {
        assert!(probe(&mut named(text.as_bytes(), "a.html")), "{text:?}");
    }
    assert!(!probe(&mut named(b"<html>\0\x01", "a.html")));
}

#[test]
fn the_language_is_the_extension_s_or_the_text_s() {
    let cases = [
        ("a.HTML", Language::Html),
        ("App.vue", Language::Html),
        ("a.phtml", Language::Php),
        ("a.scss", Language::Scss),
        ("a.less", Language::Less),
        ("a.cjs", Language::Js),
        ("a.jsx", Language::Jsx),
        ("a.mts", Language::Ts),
        ("a.tsx", Language::Tsx),
        ("a.xhtml", Language::Html),
    ];
    for (name, language) in cases {
        assert_eq!(Language::of(name, b""), language, "{name}");
    }
    assert_eq!(Language::of("page", b"<p><?= $x ?></p>"), Language::Php);
    assert_eq!(Language::of("page", b"\n  <!doctype html>"), Language::Html);
    assert_eq!(Language::of("page", b"const a = 1;"), Language::Js);
    assert_eq!(Language::from_extension("json"), None);
    let viewer = open(named(b"<?php echo 1; ?>", "noext"));
    assert_eq!(viewer.language(), Language::Php);
}

#[test]
fn the_text_is_the_file_after_its_byte_order_mark_and_no_token_covers_a_cr() {
    let text = "\u{feff}a {\r\n  color: red;\r\n}\r\n";
    let mut viewer = open(named(text.as_bytes(), "a.css"));
    assert_eq!(viewer.line_count(), 3);
    assert_eq!(text_of(&mut viewer), b"a {\r\n  color: red;\r\n}\r\n");
    let lines = highlighted("a.css", text.as_bytes());
    assert_eq!(
        lines[0].first(),
        Some(&(TokenClass::TypeName, "a".to_owned()))
    );
    assert!(
        lines.iter().flatten().all(|(_, text)| !text.contains('\r')),
        "{lines:?}"
    );
}

#[test]
fn bytes_that_are_not_utf8_are_written_as_the_replacement_character() {
    let bytes = b"x = '\xFF\xE2\x82';\nok\xC3";
    let mut viewer = open(named(bytes, "a.js"));
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(
        std::string::String::from_utf8(text_of(&mut viewer)).expect("UTF-8"),
        "x = '\u{FFFD}\u{FFFD}';\nok\u{FFFD}"
    );
    let lines = highlighted("a.js", bytes);
    assert!(lines[0].contains(&(TokenClass::String, "'\u{FFFD}\u{FFFD}'".to_owned())));
}

#[test]
fn only_lines_that_exist_get_tokens() {
    let mut viewer = open(named(b"a = 1\nb = 2", "a.js"));
    assert_eq!(viewer.line_count(), 2);
    assert_eq!(viewer.tokens(1, 10).expect("tokens").len(), 1);
    assert!(viewer.tokens(2, 10).expect("tokens").is_empty());
    assert!(viewer.tokens(0, 0).expect("tokens").is_empty());
    let mut empty = open(named(b"", "a.js"));
    assert_eq!(empty.line_count(), 0);
    assert!(empty.tokens(0, 1).expect("tokens").is_empty());
}

/// Records, a 100 KiB line in a script, a template string, a heredoc, an HTML comment, a CSS
/// comment and a start tag each over 200 lines and more, more records: checkpoints by lines and
/// by bytes, and in every state a line can start in.
fn mixed_document() -> std::string::String {
    let mut text = std::string::String::from("<!DOCTYPE html>\n");
    for index in 0..300 {
        text.push_str(&RECORD.replace("row", &format!("row{index}")));
    }
    text.push_str(&format!(
        "<script>\nconst big = \"{}\";\n</script>\n",
        "y".repeat(100 * 1024)
    ));
    text.push_str("<script>\nconst t = `\n");
    text.push_str(&"  line ${i} of template\n".repeat(200));
    text.push_str("`;\n</script>\n<?php\n$h = <<<EOT\n");
    text.push_str(&"  heredoc $x line\n".repeat(200));
    text.push_str("EOT;\n?>\n<!--\n");
    text.push_str(&"  comment text\n".repeat(200));
    text.push_str("-->\n<style>\n/*\n");
    text.push_str(&"  css comment\n".repeat(200));
    text.push_str("*/\n</style>\n<a\n");
    text.push_str(&"  data-x=\"multi\n  value\"\n".repeat(200));
    text.push_str(">\n");
    text.push_str(&RECORD.repeat(300));
    text
}

#[test]
fn tokens_from_any_checkpoint_match_the_whole_text() {
    let text = mixed_document();
    let mut viewer = open(named(text.as_bytes(), "page.php"));
    let count = u32::try_from(viewer.line_count()).expect("a few thousand lines");
    assert_eq!(count as usize, text.lines().count());
    let whole = viewer.tokens(0, count).expect("every line");
    assert_eq!(whole.len(), count as usize);
    for index in (0..count).step_by(7).chain(count - 3..count) {
        let got = viewer.tokens(index, 1).expect("tokens");
        assert_eq!(got[0], whole[index as usize], "line {index}");
    }
    for first in [
        0,
        63,
        64,
        3001,
        3002,
        3100,
        3300,
        3500,
        3700,
        3900,
        4100,
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
    let line = |needle: &str| {
        text.lines()
            .position(|line| line.contains(needle))
            .expect("the line is there")
    };
    let class = |tokens: &[Token]| tokens.iter().map(|token| token.class).collect::<Vec<_>>();
    use crate::types::TokenClass::{Comment, Key, Operator, Punctuation};
    assert_eq!(
        class(&whole[line("  line ${i}")]),
        vec![
            TokenClass::String,
            Punctuation,
            Punctuation,
            TokenClass::String
        ]
    );
    assert_eq!(
        class(&whole[line("heredoc $x")]),
        vec![TokenClass::String, Key, TokenClass::String]
    );
    assert_eq!(class(&whole[line("comment text")]), vec![Comment]);
    assert_eq!(class(&whole[line("css comment")]), vec![Comment]);
    assert_eq!(
        class(&whole[line("data-x=")]),
        vec![Key, Operator, TokenClass::String]
    );
    assert_eq!(class(&whole[line("  value\"")]), vec![TokenClass::String]);
}

#[test]
fn moving_on_cancels_the_open() {
    let mut src = named(mixed_document().as_bytes(), "page.php");
    src.stop_after = Some(1);
    let failure = Viewer::open(src).err().expect("the open stops");
    assert_eq!(failure.kind, FailureKind::Cancelled);
}

fn large(size: u64) -> Repeated {
    let body = RECORD.as_bytes().to_vec();
    Repeated {
        head: b"<!DOCTYPE html>\n".to_vec(),
        times: size / body.len() as u64 + 1,
        body,
        tail: b"<p>end</p>\n".to_vec(),
    }
}

/// `open` reads the file once in windows of at most 1 MiB and keeps its index within budget;
/// 50 lines in the middle read a window or two, not the file.
fn large_document_stays_bounded(size: u64) {
    let content = large(size);
    let (size, times) = (content.size(), content.times);
    let mut src = TestSource::new(content);
    src.name = "big.php".to_owned();
    let mut viewer = open(src);
    assert!(viewer.src.largest_read <= OPEN_CHUNK);
    assert!(viewer.src.bytes_read <= size + 3, "one pass");
    assert!(
        viewer.index_bytes() <= INDEX_BUDGET_BYTES,
        "{}",
        viewer.index_bytes()
    );
    assert_eq!(viewer.line_count(), times * 10 + 2);

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
