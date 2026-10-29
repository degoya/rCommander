use super::*;
use crate::viewer::{
    detect,
    test_support::{file, open},
};

/// LF, CRLF, a lone CR, and a last line without a break.
const MIXED: &[u8] = b"one\ntwo\r\nthree\rfour";

const UTF16LE_BOM: &[u8] = include_bytes!("../../tests/fixtures/viewer/utf16le-bom.txt");

fn texts(page: &TextPage) -> Vec<(u64, &str)> {
    page.lines
        .iter()
        .map(|line| (line.offset, line.text.as_str()))
        .collect()
}

#[tokio::test]
async fn lines_end_at_lf_crlf_and_a_lone_cr() {
    let dir = tempfile::tempdir().expect("tempdir");
    let window = open(&file(dir.path(), "mixed.txt", MIXED)).await;
    let detected = detect(MIXED);
    let page = read_lines(&window, detected, 0, 10).await.expect("read");
    assert_eq!(
        texts(&page),
        [(0, "one"), (4, "two"), (9, "three"), (15, "four")]
    );
    assert!(page.eof);
    assert_eq!(page.end, MIXED.len() as u64);

    let first = read_lines(&window, detected, 0, 2).await.expect("read");
    assert_eq!(texts(&first), [(0, "one"), (4, "two")]);
    assert!(!first.eof);
    let next = read_lines(&window, detected, first.end, 2)
        .await
        .expect("read");
    assert_eq!(texts(&next), [(9, "three"), (15, "four")]);
}

#[tokio::test]
async fn utf16_lines_skip_the_mark_and_keep_their_offsets() {
    let dir = tempfile::tempdir().expect("tempdir");
    let window = open(&file(dir.path(), "ps.txt", UTF16LE_BOM)).await;
    let detected = detect(UTF16LE_BOM);
    let page = read_lines(&window, detected, 0, 10).await.expect("read");
    let first = "Gr\u{fc}\u{df}e aus K\u{f6}ln \u{2013} na\u{ef}ve caf\u{e9}";
    let second_at = 2 + 2 * (first.chars().count() as u64 + 2);
    assert_eq!(texts(&page), [(2, first), (second_at, "zweite Zeile")]);
    assert_eq!(
        lines_before(&window, detected, second_at, 1)
            .await
            .expect("back"),
        2
    );
    assert_eq!(
        detect_line_ending(UTF16LE_BOM, detected),
        Some(LineEnding::CrLf)
    );
}

#[tokio::test]
async fn a_crlf_across_a_block_boundary_is_one_break() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut bytes = vec![b'a'; BLOCK_SIZE - 1];
    bytes.extend_from_slice(b"\r\nb\n");
    let window = open(&file(dir.path(), "split.txt", &bytes)).await;
    let page = read_lines(&window, detect(&bytes), 0, 10)
        .await
        .expect("read");
    let offsets: Vec<u64> = page.lines.iter().map(|line| line.offset).collect();
    assert_eq!(offsets, [0, BLOCK_SIZE as u64 + 1]);
    assert_eq!(page.lines[1].text, "b");
}

#[tokio::test]
async fn a_long_line_is_cut_into_pieces_on_character_boundaries() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Three bytes per euro sign: MAX_LINE_BYTES is no multiple of three.
    let euros = "\u{20ac}".repeat(MAX_LINE_BYTES / 3 + 10);
    let text = format!("{euros}\nend\n");
    let window = open(&file(dir.path(), "long.txt", text.as_bytes())).await;
    let page = read_lines(&window, detect(text.as_bytes()), 0, 10)
        .await
        .expect("read");
    let cut = (MAX_LINE_BYTES / 3 * 3) as u64;
    let offsets: Vec<u64> = page.lines.iter().map(|line| line.offset).collect();
    assert_eq!(offsets, [0, cut, euros.len() as u64 + 1]);
    assert_eq!(page.lines[0].text.chars().count(), MAX_LINE_BYTES / 3);
    assert_eq!(page.lines[1].text.chars().count(), 10);
    assert_eq!(page.lines[2].text, "end");
}

#[tokio::test]
async fn line_starts_are_found_walking_back() {
    let dir = tempfile::tempdir().expect("tempdir");
    let window = open(&file(dir.path(), "mixed.txt", MIXED)).await;
    let detected = detect(MIXED);
    for (offset, start) in [
        (0, 0),
        (2, 0),
        (4, 4),
        (7, 4),
        (8, 4),
        (9, 9),
        (14, 9),
        (15, 15),
        (19, 15),
    ] {
        assert_eq!(
            line_start(&window, detected, offset).await.expect("start"),
            start,
            "line of byte {offset}"
        );
    }
    assert_eq!(
        lines_before(&window, detected, 15, 2).await.expect("back"),
        4
    );
    assert_eq!(
        lines_before(&window, detected, 15, 10).await.expect("back"),
        0
    );
    assert_eq!(last_page(&window, detected, 2).await.expect("end"), 9);
}

#[tokio::test]
async fn the_last_page_ignores_a_final_line_break() {
    let dir = tempfile::tempdir().expect("tempdir");
    let window = open(&file(dir.path(), "ab.txt", b"a\nb\n")).await;
    let detected = detect(b"a\nb\n");
    assert_eq!(last_page(&window, detected, 1).await.expect("end"), 2);
    assert_eq!(last_page(&window, detected, 5).await.expect("end"), 0);
}

#[test]
fn tabs_expand_to_the_next_stop() {
    assert_eq!(expand_tabs("a\tb", 4), "a   b");
    assert_eq!(expand_tabs("\tx\t", 2), "  x ");
    assert_eq!(expand_tabs("a\tb", 0), "a\tb");
}

#[test]
fn the_first_line_ending_names_the_file_s_style() {
    let utf8 = |bytes: &[u8]| detect_line_ending(bytes, detect(bytes));
    assert_eq!(utf8(b"a\nb\r\n"), Some(LineEnding::Lf));
    assert_eq!(utf8(b"a\r\nb\n"), Some(LineEnding::CrLf));
    assert_eq!(utf8(b"a\rb"), Some(LineEnding::Cr));
    assert_eq!(utf8(b"no break"), None);
}

/// A 5 GiB log: its end shows at once, reading a few blocks, never the file.
#[cfg(unix)]
#[tokio::test]
async fn the_end_of_a_five_gib_file_shows_without_reading_it() {
    use std::io::{Seek, SeekFrom, Write};

    use crate::viewer::CACHE_BLOCKS;

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("huge.log");
    let mut huge = std::fs::File::create(&path).expect("create");
    // Sparse: only the last line takes space on disk.
    let size = 5u64 << 30;
    let tail = b"\nsecond to last\nlast line\n";
    huge.seek(SeekFrom::Start(size - tail.len() as u64))
        .expect("seek");
    huge.write_all(tail).expect("write");
    drop(huge);

    let window = open(&path).await;
    assert_eq!(window.size(), size);
    let detected = detect(&window.read_at(0, 4096).await.expect("head"));
    let top = last_page(&window, detected, 2).await.expect("end");
    let page = read_lines(&window, detected, top, 2).await.expect("read");
    let shown: Vec<&str> = page.lines.iter().map(|line| line.text.as_str()).collect();
    assert_eq!(shown, ["second to last", "last line"]);
    assert!(page.eof);
    assert!(window.cached_bytes() <= CACHE_BLOCKS * BLOCK_SIZE);
}
