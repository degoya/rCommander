use std::sync::{Arc, atomic::AtomicBool};

use rc_vfs::LocalFs;

use super::*;
use crate::viewer::{
    TextEncoding, detect,
    encoding::encode_char,
    test_support::{Connections, StreamOnly, file, open},
};

fn encoded(text: &str, encoding: TextEncoding) -> Vec<u8> {
    let mut bytes = encoding.bom().to_vec();
    for c in text.chars() {
        bytes.extend(encode_char(c, encoding).expect("encodable"));
    }
    bytes
}

async fn run(
    window: &FileWindow,
    needle: &Needle,
    from: u64,
    direction: Direction,
    chunk: usize,
) -> SearchOutcome {
    let cancel = AtomicBool::new(false);
    search_chunked(window, needle, from, direction, &cancel, &mut |_| {}, chunk)
        .await
        .expect("search")
}

fn at(offset: u64, len: u64) -> SearchOutcome {
    SearchOutcome::Found(Match { offset, len })
}

#[test]
fn hex_bytes_are_read_as_people_type_them() {
    assert_eq!(parse_hex("4F 6B 0a"), Ok(vec![0x4F, 0x6B, 0x0A]));
    assert_eq!(parse_hex("4f6b0a"), Ok(vec![0x4F, 0x6B, 0x0A]));
    assert_eq!(parse_hex("0x4F,0x6B"), Ok(vec![0x4F, 0x6B]));
    assert_eq!(parse_hex("   "), Err(PatternError::Empty));
    for bad in ["4", "zz", "4f6", "0x"] {
        assert!(
            matches!(parse_hex(bad), Err(PatternError::InvalidHex { .. })),
            "{bad}"
        );
    }
    assert_eq!(PatternError::Empty.code(), "search_empty");
}

#[test]
fn a_text_needs_characters_its_encoding_has() {
    let latin = Detected {
        encoding: TextEncoding::Windows1252,
        bom_len: 0,
    };
    assert_eq!(
        Needle::text("", true, latin).err(),
        Some(PatternError::Empty)
    );
    assert!(matches!(
        Needle::text("\u{3b1}", true, latin),
        Err(PatternError::NotEncodable { .. })
    ));
}

#[test]
fn case_is_ignored_in_every_encoding() {
    for encoding in TextEncoding::ALL {
        let bytes = encoded("xx GR\u{dc}N yy gr\u{fc}n", encoding);
        let detected = Detected::chosen(encoding, &bytes);
        let any_case = Needle::text("gr\u{fc}n", false, detected).expect("needle");
        let start = detected.bom_len + 3 * encoding.unit() as u64;
        let (index, len) = any_case.find(&bytes, 0).expect("found");
        assert_eq!(index as u64, start, "{encoding:?}");
        assert_eq!(len, any_case.max_len(), "{encoding:?}");
        let exact = Needle::text("gr\u{fc}n", true, detected).expect("needle");
        let (index, _) = exact.find(&bytes, 0).expect("found");
        assert!(
            index as u64 > start,
            "{encoding:?}: only the lower case one"
        );
    }
}

#[test]
fn utf16_matches_start_on_a_code_unit_only() {
    // U+4100 twice: 00 41 00 41 - an "A" (41 00) lies at the odd offset 1 only.
    let bytes = [0x00, 0x41, 0x00, 0x41];
    let detected = Detected {
        encoding: TextEncoding::Utf16Le,
        bom_len: 0,
    };
    let text = Needle::text("A", true, detected).expect("needle");
    assert_eq!(text.find(&bytes, 0), None);
    let raw = Needle::bytes(&[0x41, 0x00]).expect("needle");
    assert_eq!(raw.find(&bytes, 0), Some((1, 2)));
}

#[tokio::test]
async fn a_match_across_chunk_boundaries_is_found_both_ways() {
    let dir = tempfile::tempdir().expect("tempdir");
    let needle = Needle::text("needle", true, detect(b"x")).expect("needle");
    for place in 0..34u64 {
        let mut bytes = vec![b'x'; 40];
        bytes[place as usize..place as usize + 6].copy_from_slice(b"needle");
        let window = open(&file(dir.path(), "hay.txt", &bytes)).await;
        let forward = run(&window, &needle, 0, Direction::Forward, 4).await;
        assert_eq!(forward, at(place, 6), "forward to {place}");
        let backward = run(&window, &needle, 40, Direction::Backward, 4).await;
        assert_eq!(backward, at(place, 6), "backward to {place}");
        let past = run(&window, &needle, place + 1, Direction::Forward, 4).await;
        assert_eq!(past, SearchOutcome::NotFound, "after {place}");
        let before = run(&window, &needle, place, Direction::Backward, 4).await;
        assert_eq!(before, SearchOutcome::NotFound, "before {place}");
    }
}

#[tokio::test]
async fn a_match_at_the_end_of_a_large_file_is_found_with_progress() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut bytes = vec![0u8; 8 * SEARCH_CHUNK + 1234];
    let size = bytes.len() as u64;
    bytes.extend_from_slice(b"\xDE\xAD\xBE\xEF");
    let window = open(&file(dir.path(), "large.bin", &bytes)).await;
    let needle = Needle::bytes(&parse_hex("de ad be ef").expect("hex")).expect("needle");
    let cancel = AtomicBool::new(false);
    let mut reports = Vec::new();
    let outcome = search(
        &window,
        &needle,
        0,
        Direction::Forward,
        &cancel,
        &mut |done| reports.push(done),
    )
    .await
    .expect("search");
    assert_eq!(outcome, at(size, 4));
    assert!(reports.len() >= 8, "{reports:?}");
    assert!(reports.windows(2).all(|pair| pair[0] < pair[1]));
    let backward = search(
        &window,
        &needle,
        size + 4,
        Direction::Backward,
        &cancel,
        &mut |_| {},
    )
    .await
    .expect("search");
    assert_eq!(backward, at(size, 4));
}

#[tokio::test]
async fn a_search_stops_when_cancelled() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bytes = vec![b'x'; 64 * 1024];
    let window = open(&file(dir.path(), "none.txt", &bytes)).await;
    let needle = Needle::bytes(b"y").expect("needle");
    let cancel = AtomicBool::new(false);
    let mut chunks = 0;
    let outcome = search_chunked(
        &window,
        &needle,
        0,
        Direction::Forward,
        &cancel,
        &mut |_| {
            chunks += 1;
            cancel.store(true, Ordering::Relaxed);
        },
        1024,
    )
    .await
    .expect("search");
    assert_eq!(outcome, SearchOutcome::Cancelled);
    assert_eq!(chunks, 1);
    let backward = run(&window, &needle, 64 * 1024, Direction::Backward, 1024).await;
    assert_eq!(backward, SearchOutcome::NotFound);
}

#[tokio::test]
async fn a_backend_that_cannot_seek_is_searched_too() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = file(dir.path(), "stream.txt", b"aaaa needle bbbb needle");
    let window = FileWindow::open(Arc::new(StreamOnly(LocalFs::new())), &path)
        .await
        .expect("open");
    let needle = Needle::text("NEEDLE", false, detect(b"a")).expect("needle");
    assert_eq!(
        run(&window, &needle, 6, Direction::Forward, 4).await,
        at(17, 6)
    );
    assert_eq!(
        run(&window, &needle, 17, Direction::Backward, 4).await,
        at(5, 6)
    );
}

/// Backwards over a backend that can only stream, the one forward pass finds what the chunks
/// backwards find, across every chunk boundary (RC-080-46).
#[tokio::test]
async fn a_backward_search_over_a_stream_reads_it_once() {
    let dir = tempfile::tempdir().expect("tempdir");
    let needle = Needle::text("needle", true, detect(b"x")).expect("needle");
    for place in 0..34u64 {
        let mut bytes = vec![b'x'; 40];
        bytes[place as usize..place as usize + 6].copy_from_slice(b"needle");
        let path = file(dir.path(), "hay.txt", &bytes);
        let window = FileWindow::open(Arc::new(StreamOnly(LocalFs::new())), &path)
            .await
            .expect("open");
        let backward = run(&window, &needle, 40, Direction::Backward, 4).await;
        assert_eq!(backward, at(place, 6), "backward to {place}");
        let before = run(&window, &needle, place, Direction::Backward, 4).await;
        assert_eq!(before, SearchOutcome::NotFound, "before {place}");
        let just_after = run(&window, &needle, place + 1, Direction::Backward, 4).await;
        assert_eq!(just_after, at(place, 6), "just after {place}");
    }
    let path = file(dir.path(), "two.txt", b"needle xx needle yy needle");
    let window = FileWindow::open(Arc::new(StreamOnly(LocalFs::new())), &path)
        .await
        .expect("open");
    assert_eq!(
        run(&window, &needle, 20, Direction::Backward, 4).await,
        at(10, 6)
    );
}

/// `path` on a backend that starts reads at an offset but cannot seek, as FTP; the window has
/// read once at an offset, so it knows what the backend can (as the viewer has shown the end).
async fn over_ftp(path: &std::path::Path) -> (Arc<Connections>, FileWindow) {
    let vfs = Arc::new(Connections::default());
    let window = FileWindow::open(Arc::clone(&vfs) as Arc<dyn rc_vfs::Vfs>, path)
        .await
        .expect("open");
    window.read_uncached(1, 1).await.expect("read");
    assert!(window.resumes_at_offsets());
    (vfs, window)
}

/// Backwards over FTP, spans growing to [`MAX_SPAN_CHUNKS`] chunks are read on one data
/// connection each, not one per chunk; the match is the one the local search finds (RC-080-57).
#[tokio::test]
async fn a_backward_search_over_ftp_opens_a_connection_per_span() {
    let dir = tempfile::tempdir().expect("tempdir");
    let chunk = 256;
    let mut bytes = vec![b'x'; 256 * chunk];
    bytes[100..106].copy_from_slice(b"needle");
    let size = bytes.len() as u64;
    let path = file(dir.path(), "far.txt", &bytes);
    let needle = Needle::text("needle", true, detect(b"x")).expect("needle");
    let local = run(
        &open(&path).await,
        &needle,
        size,
        Direction::Backward,
        chunk,
    )
    .await;
    assert_eq!(local, at(100, 6));
    let (vfs, window) = over_ftp(&path).await;
    let opened = vfs.opened();
    let cancel = AtomicBool::new(false);
    let mut reports = Vec::new();
    let outcome = search_chunked(
        &window,
        &needle,
        size,
        Direction::Backward,
        &cancel,
        &mut |done| reports.push(done),
        chunk,
    )
    .await
    .expect("search");
    assert_eq!(outcome, local);
    // Spans of 2, 4, 8, 16 chunks and then 32 each: 12 connections for 256 chunks.
    let connections = vfs.opened() - opened;
    assert_eq!(connections, 12, "{connections} connections");
    assert!(
        reports.windows(2).all(|pair| pair[0] <= pair[1]),
        "{reports:?}"
    );
    assert!(
        reports.last().is_some_and(|&done| done <= size),
        "{reports:?}"
    );
}

/// A match on every place of a small file, across every span boundary, is found backwards over
/// FTP, and nothing before it; of two matches in one span the later one (RC-080-57).
#[tokio::test]
async fn a_backward_search_over_ftp_finds_a_match_across_span_boundaries() {
    let dir = tempfile::tempdir().expect("tempdir");
    let needle = Needle::text("needle", true, detect(b"x")).expect("needle");
    // A chunk is the needle's length, 6: from the end the spans start at 52, 28 and 0.
    for place in 0..58u64 {
        let mut bytes = vec![b'x'; 64];
        bytes[place as usize..place as usize + 6].copy_from_slice(b"needle");
        let path = file(dir.path(), "hay.txt", &bytes);
        let (_, window) = over_ftp(&path).await;
        let backward = run(&window, &needle, 64, Direction::Backward, 4).await;
        assert_eq!(backward, at(place, 6), "backward to {place}");
        let before = run(&window, &needle, place, Direction::Backward, 4).await;
        assert_eq!(before, SearchOutcome::NotFound, "before {place}");
        let just_after = run(&window, &needle, place + 1, Direction::Backward, 4).await;
        assert_eq!(just_after, at(place, 6), "just after {place}");
    }
    let path = file(dir.path(), "two.txt", b"needle xx needle yy needle");
    let (_, window) = over_ftp(&path).await;
    assert_eq!(
        run(&window, &needle, 20, Direction::Backward, 4).await,
        at(10, 6)
    );
}

/// Cancelled in the middle of a span, the backward search over FTP stops before the next chunk.
#[tokio::test]
async fn a_backward_search_over_ftp_stops_inside_a_span_when_cancelled() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bytes = vec![b'x'; 64 * 1024];
    let path = file(dir.path(), "none.txt", &bytes);
    let (vfs, window) = over_ftp(&path).await;
    let opened = vfs.opened();
    let needle = Needle::bytes(b"y").expect("needle");
    let cancel = AtomicBool::new(false);
    let mut reports = Vec::new();
    let outcome = search_chunked(
        &window,
        &needle,
        64 * 1024,
        Direction::Backward,
        &cancel,
        &mut |done| {
            reports.push(done);
            // After the first of the first span's two chunks.
            cancel.store(true, Ordering::Relaxed);
        },
        1024,
    )
    .await
    .expect("search");
    assert_eq!(outcome, SearchOutcome::Cancelled);
    assert_eq!(reports, vec![1024]);
    assert_eq!(vfs.opened() - opened, 1);
}
