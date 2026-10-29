use super::{CHUNK, ContentMatcher, ContentQuery, Scan, TextEncoding, Window};
use crate::search::SearchCancel;

fn matcher(text: &str, adjust: impl FnOnce(&mut ContentQuery)) -> ContentMatcher {
    let mut query = ContentQuery {
        text: text.to_owned(),
        ..ContentQuery::default()
    };
    adjust(&mut query);
    ContentMatcher::new(&query).expect("valid query")
}

async fn scan(matcher: &ContentMatcher, bytes: &[u8]) -> Scan {
    let mut reader = bytes;
    matcher
        .scan(&mut reader, &mut Vec::new(), &SearchCancel::default())
        .await
        .expect("reading a slice cannot fail")
}

/// Feeds `bytes` in two parts split at `at`, then the end, as `scan` does with its chunks.
fn found_split(matcher: &ContentMatcher, bytes: &[u8], at: usize) -> bool {
    let mut windows: Vec<Window> = matcher
        .encodings
        .iter()
        .map(|encoding| Window::new(encoding.decoder(bytes)))
        .collect();
    let parts: [(&[u8], bool); 3] = [(&bytes[..at], false), (&bytes[at..], false), (&[], true)];
    parts.iter().any(|(part, last)| {
        windows
            .iter_mut()
            .any(|window| window.feed(part, *last, matcher))
    })
}

fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn utf16be(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

const GREETING_1252: &[u8] = b"say Gr\xfc\xdfe to everyone";
const GREETING: &str = "Gr\u{fc}\u{df}e";

#[tokio::test]
async fn finds_text_in_utf8_utf16_and_windows_1252() {
    let auto = matcher("needle", |_| {});
    assert_eq!(scan(&auto, b"hay needle hay").await, Scan::Found);
    assert_eq!(scan(&auto, b"hay hay").await, Scan::Absent);

    let mut with_bom = vec![0xff, 0xfe];
    with_bom.extend(utf16le("hay needle hay"));
    assert_eq!(
        scan(&auto, &with_bom).await,
        Scan::Found,
        "auto reads the BOM"
    );

    let le = matcher("needle", |q| q.encodings = vec![TextEncoding::Utf16Le]);
    assert_eq!(scan(&le, &utf16le("hay needle")).await, Scan::Found);
    let be = matcher("needle", |q| q.encodings = vec![TextEncoding::Utf16Be]);
    assert_eq!(scan(&be, &utf16be("hay needle")).await, Scan::Found);

    let ansi = matcher(GREETING, |q| q.encodings = vec![TextEncoding::Windows1252]);
    assert_eq!(scan(&ansi, GREETING_1252).await, Scan::Found);
    // Automatic reads it as the viewer does: not UTF-8, so Windows-1252.
    let auto_greeting = matcher(GREETING, |_| {});
    assert_eq!(
        scan(&auto_greeting, GREETING_1252).await,
        Scan::Found,
        "ANSI"
    );
}

#[tokio::test]
async fn auto_detects_utf16_without_a_bom_as_the_viewer_does() {
    let auto = matcher(GREETING, |q| q.skip_binary = true);
    let bytes = utf16le("say Gr\u{fc}\u{df}e to everyone");
    assert_eq!(
        crate::viewer::detect(&bytes).encoding,
        crate::viewer::TextEncoding::Utf16Le
    );
    assert_eq!(
        scan(&auto, &bytes).await,
        Scan::Found,
        "UTF-16 is not binary"
    );
    assert_eq!(
        scan(&auto, &utf16be("hay Gr\u{fc}\u{df}e")).await,
        Scan::Found
    );
}

#[tokio::test]
async fn several_encodings_are_searched_in_one_pass() {
    let all = matcher(GREETING, |q| {
        q.encodings = vec![
            TextEncoding::Utf8,
            TextEncoding::Utf16Le,
            TextEncoding::Windows1252,
        ];
    });
    assert_eq!(scan(&all, GREETING_1252).await, Scan::Found);
    assert_eq!(scan(&all, &utf16le(GREETING)).await, Scan::Found);
    assert_eq!(scan(&all, GREETING.as_bytes()).await, Scan::Found);
}

#[test]
fn a_match_across_any_chunk_boundary_is_found() {
    let cases: [(ContentMatcher, Vec<u8>); 4] = [
        (
            matcher("needle", |_| {}),
            b"some hay, a needle, more hay".to_vec(),
        ),
        (
            matcher("needle", |q| q.encodings = vec![TextEncoding::Utf16Le]),
            utf16le("some hay, a needle, more hay"),
        ),
        (
            matcher(GREETING, |q| q.encodings = vec![TextEncoding::Windows1252]),
            GREETING_1252.to_vec(),
        ),
        (
            matcher(GREETING, |_| {}),
            format!("x {GREETING} y").into_bytes(),
        ),
    ];
    for (matcher, bytes) in &cases {
        for at in 0..=bytes.len() {
            assert!(
                found_split(matcher, bytes, at),
                "split at {at} of {bytes:?}"
            );
        }
    }
}

#[tokio::test]
async fn a_match_across_a_real_chunk_is_found() {
    let mut bytes = vec![b'x'; CHUNK - 3];
    bytes.extend(b"needle");
    bytes.extend(vec![b'y'; CHUNK]);
    assert_eq!(scan(&matcher("needle", |_| {}), &bytes).await, Scan::Found);
    let mut wide = utf16le(&"x".repeat(CHUNK / 2 - 1));
    wide.extend(utf16le("needle"));
    let le = matcher("needle", |q| q.encodings = vec![TextEncoding::Utf16Le]);
    assert_eq!(scan(&le, &wide).await, Scan::Found);
}

#[tokio::test]
async fn case_and_whole_words() {
    let any_case = matcher("NeEdLe", |_| {});
    assert_eq!(scan(&any_case, b"a needle").await, Scan::Found);
    let exact = matcher("NeEdLe", |q| q.case_sensitive = true);
    assert_eq!(scan(&exact, b"a needle").await, Scan::Absent);
    let word = matcher("need", |q| q.whole_words = true);
    assert_eq!(scan(&word, b"needle").await, Scan::Absent);
    assert_eq!(scan(&word, b"in need.").await, Scan::Found);
    assert_eq!(scan(&word, b"need").await, Scan::Found);
}

#[test]
fn a_whole_word_is_not_found_in_a_longer_word_split_after_it() {
    let word = matcher("foo", |q| q.whole_words = true);
    for at in 0..=9 {
        assert!(!found_split(&word, b"x foobar y", at), "split at {at}");
        assert!(found_split(&word, b"x foo bar", at), "split at {at}");
    }
}

#[tokio::test]
async fn regular_expressions() {
    let re = matcher(r"err(or)?\s+\d{3}", |q| q.regex = true);
    assert_eq!(scan(&re, b"line\nerror 404\n").await, Scan::Found);
    assert_eq!(scan(&re, b"error code").await, Scan::Absent);
    let line = matcher(r"^end$", |q| q.regex = true);
    assert_eq!(scan(&line, b"start\nend\nmore").await, Scan::Found);
    assert!(
        !found_split(&line, b"start\nendless", 9),
        "$ waits for what follows"
    );
    for at in 0..=15 {
        assert!(
            found_split(&re, b"pre error  500 post", at),
            "split at {at}"
        );
    }
}

#[test]
fn a_broken_regex_or_an_empty_text_is_an_invalid_search() {
    let broken = ContentQuery {
        text: "(".to_owned(),
        regex: true,
        ..ContentQuery::default()
    };
    let empty = ContentQuery::default();
    for query in [broken, empty] {
        let error = ContentMatcher::new(&query).expect_err("invalid");
        assert_eq!(error.code(), "invalid_search");
    }
}

#[tokio::test]
async fn binary_files_are_skipped_when_asked() {
    let skip = matcher("needle", |q| q.skip_binary = true);
    assert_eq!(scan(&skip, b"\x7fELF\0\0needle").await, Scan::Binary);
    assert_eq!(scan(&skip, b"plain needle").await, Scan::Found);
    let mut with_bom = vec![0xff, 0xfe];
    with_bom.extend(utf16le("needle"));
    assert_eq!(
        scan(&skip, &with_bom).await,
        Scan::Found,
        "UTF-16 is not binary"
    );
    let keep = matcher("needle", |_| {});
    assert_eq!(scan(&keep, b"\x7fELF\0\0needle").await, Scan::Found);
}

#[tokio::test]
async fn a_cancelled_scan_stops() {
    let cancel = SearchCancel::default();
    cancel.cancel();
    let mut reader: &[u8] = b"needle";
    let scan = matcher("needle", |_| {})
        .scan(&mut reader, &mut Vec::new(), &cancel)
        .await
        .expect("scan");
    assert_eq!(scan, Scan::Cancelled);
}

#[test]
fn auto_decodes_every_encoding_as_the_viewer_does() {
    let mut with_bom = vec![0xef, 0xbb, 0xbf];
    with_bom.extend_from_slice(GREETING.as_bytes());
    let cases: [Vec<u8>; 5] = [
        GREETING.as_bytes().to_vec(),
        with_bom,
        utf16le(GREETING),
        utf16be("say Gr\u{fc}\u{df}e"),
        b"\x80 \x93quoted\x94 Gr\xfc\xdfe".to_vec(),
    ];
    for bytes in cases {
        let mut decoder = TextEncoding::Auto.decoder(&bytes);
        let mut text = String::with_capacity(
            decoder
                .max_utf8_buffer_length(bytes.len())
                .expect("small input"),
        );
        let _ = decoder.decode_to_string(&bytes, &mut text, true);
        assert_eq!(text, crate::viewer::decode_text(&bytes).1, "{bytes:?}");
    }
}
