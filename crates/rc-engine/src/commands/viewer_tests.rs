use std::fs;

use tempfile::TempDir;

use crate::test_support;
use crate::{
    Command, Engine, Outcome,
    viewer::{HexWindow, ReadHexRequest, ReadTextRequest, TextEncoding, TextWindow},
};

const UTF16LE_BOM: &[u8] = include_bytes!("../../tests/fixtures/viewer/utf16le-bom.txt");
const CP1252: &[u8] = include_bytes!("../../tests/fixtures/viewer/cp1252.txt");
const BYTES: &[u8] = include_bytes!("../../tests/fixtures/viewer/bytes.bin");

fn put(root: &TempDir, name: &str, bytes: &[u8]) -> String {
    fs::write(root.path().join(name), bytes).expect("write");
    name.to_owned()
}

fn text_request(path: impl Into<String>) -> ReadTextRequest {
    ReadTextRequest {
        path: path.into(),
        panel: None,
        offset: 0,
        max_bytes: None,
        encoding: None,
    }
}

async fn read_text(engine: &Engine, request: ReadTextRequest) -> TextWindow {
    match engine.dispatch_mcp(Command::ReadText(request)).await {
        Ok(Outcome::FileText(window)) => window,
        other => panic!("a text window, got {other:?}"),
    }
}

async fn read_hex(engine: &Engine, path: &str, offset: u64, length: Option<u64>) -> HexWindow {
    let request = ReadHexRequest {
        path: path.to_owned(),
        panel: None,
        offset,
        length,
    };
    match engine.dispatch_mcp(Command::ReadHex(request)).await {
        Ok(Outcome::FileHex(window)) => window,
        other => panic!("a hex window, got {other:?}"),
    }
}

#[tokio::test]
async fn a_small_file_reads_whole_in_its_detected_encoding() {
    let root = tempfile::tempdir().expect("tempdir");
    let engine = test_support::headless(root.path()).await;
    let name = put(&root, "ps.txt", UTF16LE_BOM);
    let window = read_text(&engine, text_request(name)).await;
    assert_eq!(window.encoding, TextEncoding::Utf16Le);
    assert!(window.bom);
    assert_eq!((window.start, window.end), (2, UTF16LE_BOM.len() as u64));
    assert!(!window.truncated && !window.binary && !window.masked);
    assert!(window.text.starts_with("Gr\u{fc}\u{df}e aus K\u{f6}ln"));
    assert_eq!(window.path.path, root.path().join("ps.txt"));
}

#[tokio::test]
async fn the_limit_cuts_the_text_and_says_so() {
    let root = tempfile::tempdir().expect("tempdir");
    let engine = test_support::headless(root.path()).await;
    let name = put(&root, "big.log", &b"0123456789abcde\n".repeat(320 * 1024));
    let size = 320 * 1024 * 16;

    let first = read_text(&engine, text_request(name.clone())).await;
    assert_eq!(first.end - first.start, 256 * 1024);
    assert!(first.truncated);
    let mut next = text_request(name.clone());
    next.offset = first.end;
    next.max_bytes = Some(u64::MAX);
    let rest = read_text(&engine, next).await;
    assert_eq!(rest.start, first.end);
    assert_eq!(rest.end - rest.start, 4 * 1024 * 1024, "capped at 4 MiB");
    assert!(rest.truncated);

    let mut tail = text_request(name.clone());
    tail.offset = size - 16;
    let tail = read_text(&engine, tail).await;
    assert_eq!(tail.text, "0123456789abcde\n");
    assert!(!tail.truncated);

    let mut one = text_request(name);
    one.max_bytes = Some(0);
    assert_eq!(read_text(&engine, one).await.text, "0");
}

#[tokio::test]
async fn an_encoding_can_be_chosen() {
    let root = tempfile::tempdir().expect("tempdir");
    let engine = test_support::headless(root.path()).await;
    let name = put(&root, "ansi.txt", CP1252);
    let detected = read_text(&engine, text_request(name.clone())).await;
    assert_eq!(detected.encoding, TextEncoding::Windows1252);
    assert!(detected.text.contains('\u{20ac}'));
    let mut latin1 = text_request(name);
    latin1.encoding = Some(TextEncoding::Latin1);
    let chosen = read_text(&engine, latin1).await;
    assert_eq!(chosen.encoding, TextEncoding::Latin1);
    assert!(chosen.text.contains('\u{80}'));
}

#[tokio::test]
async fn a_file_of_the_dos_extensions_reads_as_cp437() {
    let root = tempfile::tempdir().expect("tempdir");
    let engine = test_support::headless(root.path()).await;
    let art = b"\xC9\xCD\xBB Gr\x81\xE1e\r\n";
    let nfo = read_text(&engine, text_request(put(&root, "release.nfo", art))).await;
    assert_eq!(nfo.encoding, TextEncoding::Cp437);
    assert_eq!(nfo.text, "\u{2554}\u{2550}\u{2557} Gr\u{fc}\u{df}e\r\n");
    let txt = read_text(&engine, text_request(put(&root, "release.txt", art))).await;
    assert_eq!(txt.encoding, TextEncoding::Windows1252);

    let db = rc_db::Db::open_in_memory().await.expect("db");
    db.set(crate::settings::DOS_EXTENSIONS_KEY, &"txt")
        .await
        .expect("set");
    assert!(engine.attach_db(db));
    let txt = read_text(&engine, text_request("release.txt")).await;
    assert_eq!(txt.encoding, TextEncoding::Cp437, "the setting names it");
    let nfo = read_text(&engine, text_request("release.nfo")).await;
    assert_eq!(nfo.encoding, TextEncoding::Windows1252, "no longer named");
}

#[tokio::test]
async fn hex_rows_follow_xxd_from_the_offset() {
    let root = tempfile::tempdir().expect("tempdir");
    let engine = test_support::headless(root.path()).await;
    let name = put(&root, "bytes.bin", BYTES);
    let window = read_hex(&engine, &name, 0x70, Some(32)).await;
    assert_eq!(
        window.rows,
        [
            "00000070: 7071 7273 7475 7677 7879 7a7b 7c7d 7e7f  pqrstuvwxyz{|}~.",
            "00000080: 8081 8283 8485 8687 8889 8a8b 8c8d 8e8f  ................",
        ]
    );
    assert_eq!((window.start, window.end), (0x70, 0x90));
    assert!(window.truncated && !window.masked);
    let whole = read_hex(&engine, &name, 0, None).await;
    assert_eq!(whole.rows.len(), 17);
    assert!(!whole.truncated);
}

#[tokio::test]
async fn a_private_key_never_leaves() {
    let root = tempfile::tempdir().expect("tempdir");
    let engine = test_support::headless(root.path()).await;
    let begin = format!("-----BEGIN OPENSSH {}-----\n", "PRIVATE KEY");
    let end = format!("-----END OPENSSH {}-----\n", "PRIVATE KEY");
    let key = format!("{begin}b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQ\n{end}");
    let name = put(&root, "id_ed25519", key.as_bytes());
    // From the middle of the body: the window alone would not show a marker.
    let mut inside = text_request(name.clone());
    inside.offset = begin.len() as u64 + 5;
    inside.max_bytes = Some(10);
    let text = read_text(&engine, inside).await;
    assert!(text.masked);
    assert_eq!(text.text, "[private key redacted]");
    let hex = read_hex(&engine, &name, 0, None).await;
    assert!(hex.masked && hex.rows.is_empty());

    // A key inside a larger file is masked in the window that shows it.
    let mut big = "filler line\n".repeat(2000);
    big.push_str(&key);
    let name = put(&root, "bundle.pem", big.as_bytes());
    let mut window = text_request(name);
    window.offset = 2000 * 12 - 12;
    let text = read_text(&engine, window).await;
    assert!(text.masked);
    assert_eq!(text.text, "filler line\n[private key redacted]\n");
}

#[tokio::test]
async fn a_directory_or_a_missing_file_is_an_error_with_its_code() {
    let root = tempfile::tempdir().expect("tempdir");
    let engine = test_support::headless(root.path()).await;
    fs::create_dir(root.path().join("sub")).expect("mkdir");
    for (path, code) in [("sub", "is_a_directory"), ("none.txt", "not_found")] {
        let error = engine
            .dispatch_mcp(Command::ReadText(text_request(path)))
            .await
            .expect_err("refused");
        assert_eq!(error.code(), code, "{path}");
    }
    let absolute = root.path().join("sub").join("..").join("sub");
    let error = engine
        .dispatch(Command::ReadHex(ReadHexRequest {
            path: absolute.to_string_lossy().into_owned(),
            panel: None,
            offset: 0,
            length: None,
        }))
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "is_a_directory");
}

/// A file far larger than a key file with a key in the middle: 20 KiB of text, the key (BEGIN
/// line, a 3000-byte body, END line), 20 KiB of text. The offset of the body's middle.
fn key_in_the_middle(root: &TempDir) -> (String, u64) {
    let filler = "plain text line\n".repeat(1280);
    let body = "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFB\n".repeat(44);
    let edge = |edge: &str| format!("-----{edge} OPENSSH {}-----\n", "PRIVATE KEY");
    let text = format!("{filler}{}{body}{}{filler}", edge("BEGIN"), edge("END"));
    let middle = (filler.len() + edge("BEGIN").len() + body.len() / 2) as u64;
    (put(root, "big.log", text.as_bytes()), middle)
}

/// A window of nothing but a key's body carries no marker; the bytes around it tell, and it is
/// masked whole as text and as hex. A window far from the key is not.
#[tokio::test]
async fn a_window_inside_a_key_body_is_masked() {
    let root = tempfile::tempdir().expect("tempdir");
    let engine = test_support::headless(root.path()).await;
    let (name, middle) = key_in_the_middle(&root);
    let text = read_text(
        &engine,
        ReadTextRequest {
            offset: middle,
            max_bytes: Some(256),
            ..text_request(name.clone())
        },
    )
    .await;
    assert!(text.masked);
    assert!(!text.text.contains("QUFB"), "{}", text.text);
    let hex = read_hex(&engine, &name, middle, Some(256)).await;
    assert!(hex.masked);
    assert!(hex.rows.is_empty());
    let far = read_text(
        &engine,
        ReadTextRequest {
            max_bytes: Some(256),
            ..text_request(name)
        },
    )
    .await;
    assert!(!far.masked);
    assert!(far.text.starts_with("plain text line"));
}
