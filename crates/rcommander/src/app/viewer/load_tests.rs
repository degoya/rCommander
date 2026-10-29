use std::sync::{Arc, atomic::AtomicBool};

use iced::futures::StreamExt;
use rc_engine::{
    VfsPath,
    viewer::{Direction, LineEnding, Match, Needle, SearchOutcome, TextEncoding, ViewMode},
};
use tempfile::TempDir;

use super::{Found, Layout, Opened, Seek, Span, binary_line, open, page, search, shown};
use crate::app::harness::headless_engine;

async fn opened(bytes: &[u8]) -> (TempDir, Opened) {
    let root = tempfile::tempdir().expect("tempdir");
    std::fs::write(root.path().join("f"), bytes).expect("write");
    let engine = headless_engine(root.path()).await;
    let file = open(engine, VfsPath::local(root.path().join("f")), None)
        .await
        .expect("opens");
    (root, file)
}

fn layout(file: &Opened, mode: ViewMode, rows: usize) -> Layout {
    Layout {
        mode,
        detected: file.detected,
        tab_width: 8,
        left: 0,
        rows,
    }
}

fn numbered(count: usize) -> Vec<u8> {
    (0..count)
        .map(|n| format!("line {n}\r\n"))
        .collect::<String>()
        .into_bytes()
}

#[tokio::test]
async fn opening_tells_encoding_line_ending_and_mode() {
    let (_root, file) = opened(b"\xFF\xFEa\x00\r\x00\n\x00").await;
    assert_eq!(file.detected.encoding, TextEncoding::Utf16Le);
    assert_eq!(file.ending, Some(LineEnding::CrLf));
    assert_eq!(file.mode, ViewMode::Text);
    let (_root, png) = opened(b"\x89PNG\r\n\x1a\n\0\0\0\0").await;
    assert_eq!(png.mode, ViewMode::Image);
}

#[tokio::test]
async fn text_pages_scroll_down_up_and_to_the_end() {
    let (_root, file) = opened(&numbered(100)).await;
    let view = layout(&file, ViewMode::Text, 10);
    let first = page(
        (file.file.clone(), file.last_page.clone()),
        view,
        0,
        Seek::Stay,
        None,
    )
    .await
    .expect("a page");
    assert_eq!(first.lines.len(), 10);
    assert_eq!(first.lines[0], "line 0");
    let down = page(
        (file.file.clone(), file.last_page.clone()),
        view,
        first.top,
        Seek::Down(3),
        None,
    )
    .await
    .expect("down");
    assert_eq!(down.lines[0], "line 3");
    let up = page(
        (file.file.clone(), file.last_page.clone()),
        view,
        down.top,
        Seek::Up(2),
        None,
    )
    .await
    .expect("up");
    assert_eq!(up.lines[0], "line 1");
    let end = page(
        (file.file.clone(), file.last_page.clone()),
        view,
        up.top,
        Seek::End,
        None,
    )
    .await
    .expect("end");
    assert_eq!(end.lines.first().map(String::as_str), Some("line 90"));
    assert_eq!(end.lines.last().map(String::as_str), Some("line 99"));
    let past = page(
        (file.file.clone(), file.last_page.clone()),
        view,
        end.top,
        Seek::Down(5),
        None,
    )
    .await
    .expect("stays");
    assert_eq!(past.top, end.top, "the last page does not scroll on");
}

#[tokio::test]
async fn a_found_match_is_marked_on_its_line_with_tabs_expanded() {
    let (_root, file) = opened(b"one\n\tneedle here\nthree\n").await;
    let view = layout(&file, ViewMode::Text, 5);
    let found = Match { offset: 5, len: 6 };
    let shown_page = page(
        (file.file.clone(), file.last_page.clone()),
        view,
        0,
        Seek::At(5),
        Some(found),
    )
    .await
    .expect("a page");
    assert_eq!(shown_page.lines[0], "        needle here");
    assert_eq!(
        shown_page.mark,
        Some(Span {
            from: (0, 8),
            to: (0, 14)
        })
    );
}

/// A match that starts inside a character - a hex search for `A4 79` in `x`, U+00E4, `y` - marks that
/// whole character; the mark ended at the line's end before, between nothing (RC-080-44).
#[tokio::test]
async fn a_match_inside_a_character_marks_the_whole_character() {
    let (_root, file) = opened("x\u{e4}y\n".as_bytes()).await;
    let view = layout(&file, ViewMode::Text, 5);
    let found = Match { offset: 2, len: 2 };
    let shown_page = page(
        (file.file.clone(), file.last_page.clone()),
        view,
        0,
        Seek::At(0),
        Some(found),
    )
    .await
    .expect("a page");
    assert_eq!(
        shown_page.mark,
        Some(Span {
            from: (0, 1),
            to: (0, 4)
        })
    );
}

#[tokio::test]
async fn hex_rows_are_xxd_rows_and_mark_their_bytes() {
    let bytes: Vec<u8> = (0..=255).collect();
    let (_root, file) = opened(&bytes).await;
    let view = layout(&file, ViewMode::Hex, 4);
    let found = Match {
        offset: 0x21,
        len: 2,
    };
    let rows = page(
        (file.file.clone(), file.last_page.clone()),
        view,
        0,
        Seek::At(0x20),
        Some(found),
    )
    .await
    .expect("rows");
    assert_eq!(rows.top, 0x20);
    assert_eq!(
        rows.lines[0],
        "00000020: 2021 2223 2425 2627 2829 2a2b 2c2d 2e2f   !\"#$%&'()*+,-./"
    );
    assert_eq!(
        rows.mark,
        Some(Span {
            from: (0, 12),
            to: (0, 17)
        })
    );
    let end = page(
        (file.file.clone(), file.last_page.clone()),
        view,
        0,
        Seek::End,
        None,
    )
    .await
    .expect("end");
    assert_eq!(end.top, 0xC0);
    assert_eq!(end.end, 256);
}

#[test]
fn control_characters_show_as_dots() {
    assert_eq!(binary_line(b"A\0\x80\n", TextEncoding::Utf8), "A.\u{20AC}.");
    assert_eq!(binary_line(b"\xE9", TextEncoding::Latin1), "\u{e9}");
    let view = Layout {
        mode: ViewMode::Text,
        detected: rc_engine::viewer::detect(b"x"),
        tab_width: 4,
        left: 2,
        rows: 1,
    };
    assert_eq!(shown("a\tb\x07", view), "  b.");
}

#[tokio::test]
async fn a_search_reports_progress_and_finds_the_last_line() {
    let mut bytes = numbered(50_000);
    bytes.extend_from_slice(b"the end");
    let (_root, file) = opened(&bytes).await;
    let needle = Needle::text("THE END", false, file.detected).expect("a needle");
    let news: Vec<Found> = search(
        file.file.clone(),
        needle,
        0,
        Direction::Forward,
        Arc::new(AtomicBool::new(false)),
    )
    .collect()
    .await;
    let Some(Found::Done(Ok(SearchOutcome::Found(found)))) = news.last() else {
        panic!("found at the end, got {news:?}");
    };
    assert_eq!(found.offset, bytes.len() as u64 - 7);
}

#[tokio::test]
async fn a_cancelled_search_says_so() {
    let (_root, file) = opened(&numbered(1000)).await;
    let needle = Needle::bytes(b"missing").expect("a needle");
    let news: Vec<Found> = search(
        file.file.clone(),
        needle,
        0,
        Direction::Forward,
        Arc::new(AtomicBool::new(true)),
    )
    .collect()
    .await;
    assert!(matches!(
        news.last(),
        Some(Found::Done(Ok(SearchOutcome::Cancelled)))
    ));
}
