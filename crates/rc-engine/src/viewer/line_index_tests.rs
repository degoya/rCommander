use super::*;
use crate::viewer::{
    detect,
    test_support::{file, open},
};

const UTF16LE_BOM: &[u8] = include_bytes!("../../tests/fixtures/viewer/utf16le-bom.txt");

/// The index of `bytes`, fed `chunk` bytes at a time.
fn indexed(bytes: &[u8], chunk: usize) -> LineIndex {
    let mut index = LineIndex::new(detect(bytes));
    let from = usize::try_from(index.next_offset()).expect("small");
    for piece in bytes[from..].chunks(chunk) {
        index.feed(piece);
    }
    index.finish();
    index
}

#[test]
fn every_kind_of_line_break_counts_whatever_the_chunks() {
    for chunk in [1, 2, 3, 64] {
        let index = indexed(b"one\ntwo\r\nthree\rfour", chunk);
        assert_eq!(index.line_count(), 4, "chunks of {chunk}");
        let utf16 = indexed(UTF16LE_BOM, chunk);
        assert_eq!(utf16.line_count(), 2, "UTF-16 in chunks of {chunk}");
    }
}

#[test]
fn a_final_break_starts_no_further_line() {
    assert_eq!(indexed(b"a\nb\n", 4).line_count(), 2);
    assert_eq!(indexed(b"a\nb", 4).line_count(), 2);
    assert_eq!(indexed(b"a\r", 4).line_count(), 1);
    assert_eq!(indexed(b"\n", 4).line_count(), 1);
    assert_eq!(indexed(b"", 4).line_count(), 1);
}

#[test]
fn memory_stays_bounded_for_millions_of_lines() {
    let lines = 5_000_000;
    let index = indexed(&vec![b'\n'; lines], 1 << 20);
    assert_eq!(index.line_count(), lines as u64);
    assert!(index.checkpoint_count() <= MAX_CHECKPOINTS);
    assert!(index.stride() > 64, "the stride doubled");
    // Every line is one byte long, so line n starts at byte n.
    let (line, offset) = index.checkpoint(4_999_999).expect("indexed");
    assert_eq!(line, offset);
    assert!(4_999_999 - line < index.stride());
}

#[test]
fn an_unfinished_index_knows_only_what_it_has_seen() {
    let mut index = LineIndex::new(detect(b"a\nb\nc\n"));
    index.feed(b"a\nb");
    assert!(!index.is_complete());
    assert_eq!(index.line_count(), 2);
    assert_eq!(index.checkpoint(1), Some((0, 0)));
    assert_eq!(index.checkpoint(2), None);
}

#[tokio::test]
async fn line_numbers_and_offsets_turn_into_each_other() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut bytes = Vec::new();
    let mut starts = Vec::new();
    for n in 0..1000 {
        starts.push(bytes.len() as u64);
        let ending: &[u8] = if n % 3 == 0 { b"\r\n" } else { b"\n" };
        bytes.extend_from_slice(format!("line {n}").as_bytes());
        bytes.extend_from_slice(ending);
    }
    let window = open(&file(dir.path(), "lines.txt", &bytes)).await;
    let mut index = LineIndex::new(detect(&bytes));
    while !index.step(&window, 7).await.expect("step") {}
    assert_eq!(index.line_count(), 1000);
    for line in [0, 1, 63, 64, 65, 500, 999] {
        let offset = index
            .line_offset(&window, line)
            .await
            .expect("read")
            .expect("a line");
        assert_eq!(offset, starts[line as usize], "line {line}");
        let back = index.line_at(&window, offset + 2).await.expect("read");
        assert_eq!(back, Some(line), "the line of byte {}", offset + 2);
    }
    assert_eq!(index.line_offset(&window, 1000).await.expect("read"), None);
    let last = index
        .line_at(&window, bytes.len() as u64 - 1)
        .await
        .expect("read");
    assert_eq!(last, Some(999));
}

#[tokio::test]
async fn an_unfinished_index_answers_none_beyond_its_reach() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bytes = b"a\nb\nc\nd\n".repeat(100);
    let window = open(&file(dir.path(), "abcd.txt", &bytes)).await;
    let mut index = LineIndex::new(detect(&bytes));
    assert!(!index.step(&window, 10).await.expect("step"));
    assert_eq!(index.line_at(&window, 500).await.expect("read"), None);
    assert_eq!(index.line_at(&window, 3).await.expect("read"), Some(1));
}
