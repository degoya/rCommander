//! The viewer's `update()` without a window: keys, pages and stale answers.

use iced::widget::text_editor::Action;
use rc_db::Db;
use rc_engine::{
    PanelId, VfsPath,
    settings::DOS_EXTENSIONS_KEY,
    viewer::{TextEncoding, ViewMode},
};
use tempfile::TempDir;

use super::{Lister, Request, Shown};
use crate::app::harness::headless_engine;
use crate::app::viewer::{
    keys::{ListerKey, Scroll},
    load::{self, Page, Seek},
};

async fn lister_on(bytes: &[u8]) -> (TempDir, Lister, Vec<Request>) {
    lister_named("f", bytes, None).await
}

/// A lister on the file `name` holding `bytes`, the settings read from `db`.
async fn lister_named(name: &str, bytes: &[u8], db: Option<Db>) -> (TempDir, Lister, Vec<Request>) {
    let root = tempfile::tempdir().expect("tempdir");
    let path = VfsPath::local(root.path().join(name));
    std::fs::write(&path.path, bytes).expect("write");
    let engine = headless_engine(root.path()).await;
    let opened = load::open(engine, path.clone(), db).await.expect("opens");
    let mut lister = Lister::new(1, PanelId::Left, path);
    let requests = lister.opened(opened);
    (root, lister, requests)
}

/// An `.nfo` frame's first line and a line of text with an umlaut, in CP437.
const NFO: &[u8] = b"\xC9\xCD\xCD\xBB\r\n\xBA \xDB Gr\x81\xE1e\r\n";

fn page_seek(requests: &[Request]) -> Option<(u64, Seek)> {
    match requests {
        [
            Request::Page {
                generation, seek, ..
            },
        ] => Some((*generation, *seek)),
        _ => None,
    }
}

fn page(top: u64, lines: &[&str]) -> Result<Page, &'static str> {
    Ok(Page {
        top,
        end: top + 10,
        lines: lines.iter().map(|line| (*line).to_owned()).collect(),
        mark: None,
        raw: Vec::new(),
    })
}

#[test]
fn nothing_is_read_before_the_file_is_open() {
    let mut lister = Lister::new(1, PanelId::Right, VfsPath::local("/x/y.txt"));
    assert!(lister.key(ListerKey::Scroll(Scroll::PageDown)).is_empty());
    assert_eq!(lister.name(), "y.txt");
}

#[tokio::test]
async fn a_text_file_opens_on_its_first_page_and_a_stale_page_is_dropped() {
    let (_root, mut lister, requests) = lister_on(b"one\ntwo\n").await;
    assert_eq!(page_seek(&requests), Some((1, Seek::At(0))));
    let newer = lister.key(ListerKey::Scroll(Scroll::LineDown));
    assert_eq!(page_seek(&newer), Some((2, Seek::Down(1))));
    lister.paged(1, page(0, &["stale"]));
    assert_eq!(lister.content.text(), "");
    lister.paged(2, page(4, &["two"]));
    assert_eq!(lister.content.text(), "two");
    assert_eq!(lister.top, 4);
}

#[tokio::test]
async fn mode_keys_switch_and_the_image_mode_wants_an_image() {
    let (_root, mut lister, _) = lister_on(b"plain text\n").await;
    assert!(lister.key(ListerKey::Mode(ViewMode::Image)).is_empty());
    let hex = lister.key(ListerKey::Mode(ViewMode::Hex));
    assert_eq!(lister.mode, ViewMode::Hex);
    assert!(matches!(page_seek(&hex), Some((_, Seek::At(0)))));
    let (_root, mut picture, requests) = lister_on(b"\x89PNG\r\n\x1a\n\0\0").await;
    assert!(matches!(requests.as_slice(), [Request::Picture { .. }]));
    assert!(matches!(picture.picture, Shown::Loading));
    picture.pictured(Err("image_broken"));
    assert!(matches!(picture.picture, Shown::Failed("image_broken")));
    assert!(picture.key(ListerKey::Mode(ViewMode::Image)).is_empty());
}

#[tokio::test]
async fn an_encoding_key_turns_the_view_to_text_in_that_encoding() {
    let (_root, mut lister, _) = lister_on(b"caf\xe9\n").await;
    assert_eq!(lister.detected.encoding, TextEncoding::Windows1252);
    lister.key(ListerKey::Mode(ViewMode::Hex));
    let requests = lister.key(ListerKey::Encoding(TextEncoding::Utf8));
    assert_eq!(lister.detected.encoding, TextEncoding::Utf8);
    assert_eq!(lister.mode, ViewMode::Text);
    assert!(page_seek(&requests).is_some());
    lister.key(ListerKey::NextEncoding);
    assert_eq!(lister.detected.encoding, TextEncoding::Utf16Le);
}

#[tokio::test]
async fn an_nfo_opens_in_cp437_without_wrap_and_its_art_reads_as_drawn() {
    let (_root, lister, _) = lister_named("release.nfo", NFO, None).await;
    assert_eq!(lister.detected.encoding, TextEncoding::Cp437);
    assert!(!lister.wrap, "the art is drawn for its columns");
    let opened = lister.file.as_ref().expect("open");
    let file = (opened.file.clone(), opened.last_page.clone());
    let page = load::page(file, lister.layout(), 0, Seek::At(0), None)
        .await
        .expect("a page");
    assert_eq!(
        page.lines[..2],
        [
            "\u{2554}\u{2550}\u{2550}\u{2557}",
            "\u{2551} \u{2588} Gr\u{fc}\u{df}e",
        ]
    );
    let (_root, txt, _) = lister_named("release.txt", NFO, None).await;
    assert_eq!(txt.detected.encoding, TextEncoding::Windows1252);
    assert!(txt.wrap);
}

#[tokio::test]
async fn s_and_a_switch_between_dos_and_ansi_and_e_passes_cp437() {
    let (_root, mut lister, _) = lister_on(b"caf\xe9\n").await;
    let requests = lister.key(ListerKey::Encoding(TextEncoding::Cp437));
    assert_eq!(lister.detected.encoding, TextEncoding::Cp437);
    assert!(page_seek(&requests).is_some());
    lister.key(ListerKey::Encoding(TextEncoding::Windows1252));
    lister.key(ListerKey::NextEncoding);
    assert_eq!(lister.detected.encoding, TextEncoding::Cp437, "after ANSI");
    lister.key(ListerKey::NextEncoding);
    assert_eq!(lister.detected.encoding, TextEncoding::Utf8, "round again");
}

#[tokio::test]
async fn the_setting_names_the_extensions_that_open_as_dos_text() {
    let db = Db::open_in_memory().await.expect("db");
    db.set(DOS_EXTENSIONS_KEY, &"asc").await.expect("set");
    let (_root, asc, _) = lister_named("logo.asc", NFO, Some(db.clone())).await;
    assert_eq!(asc.detected.encoding, TextEncoding::Cp437);
    assert!(!asc.wrap);
    let (_root, nfo, _) = lister_named("release.nfo", NFO, Some(db)).await;
    assert_eq!(nfo.detected.encoding, TextEncoding::Windows1252);
    assert!(nfo.wrap);
}

#[tokio::test]
async fn the_wheel_scrolls_the_view_and_never_edits() {
    let (_root, mut lister, _) = lister_on(b"a\nb\nc\n").await;
    let down = lister.editor(Action::Scroll { lines: 4 });
    assert!(matches!(page_seek(&down), Some((_, Seek::Down(4)))));
    let up = lister.editor(Action::Scroll { lines: -2 });
    assert!(matches!(page_seek(&up), Some((_, Seek::Up(2)))));
    let edit = iced::widget::text_editor::Edit::Insert('x');
    assert!(lister.editor(Action::Edit(edit)).is_empty());
}

#[tokio::test]
async fn columns_move_only_without_wrap() {
    let (_root, mut lister, _) = lister_on(b"a long line\n").await;
    assert!(lister.key(ListerKey::Scroll(Scroll::Right)).is_empty());
    lister.key(ListerKey::Wrap);
    lister.key(ListerKey::Scroll(Scroll::Right));
    assert_eq!(lister.layout().left, 8);
    lister.key(ListerKey::Wrap);
    assert_eq!(lister.layout().left, 0);
}

#[test]
fn zoom_and_turn_keys_change_the_picture_only() {
    let mut lister = Lister::new(1, PanelId::Left, VfsPath::local("/p.png"));
    lister.key(ListerKey::ZoomIn);
    assert!(!lister.zoom.fit);
    assert!((lister.zoom.scale - 1.0).abs() < f32::EPSILON);
    lister.key(ListerKey::ZoomIn);
    assert!(lister.zoom.scale > 1.0);
    lister.key(ListerKey::Fit);
    assert!(lister.zoom.fit);
    lister.key(ListerKey::RotateLeft);
    assert_eq!(lister.zoom.turns, 3);
    lister.key(ListerKey::RotateRight);
    assert_eq!(lister.zoom.turns, 0);
}

/// RC-080-53: a picture asked for before a reload (F2) that answers after it is dropped; the
/// picture asked for by the reload is shown.
#[tokio::test]
async fn a_picture_asked_before_a_reload_is_dropped() {
    let (root, mut lister, requests) = lister_on(b"\x89PNG\r\n\x1a\n\0\0").await;
    let [Request::Picture { generation: before }] = requests.as_slice() else {
        panic!("the picture, got {requests:?}");
    };
    let engine = headless_engine(root.path()).await;
    let path = VfsPath::local(root.path().join("f"));
    let opened = load::open(engine, path, None).await.expect("opens");
    let again = lister.reloaded(opened);
    let Some(Request::Picture { generation: after }) = again.first() else {
        panic!("the picture again, got {again:?}");
    };
    assert_ne!(before, after);
    lister.picture_read(*before, Err("image_broken"));
    assert!(matches!(lister.picture, Shown::Loading), "the old answer");
    lister.picture_read(*after, Err("image_too_large"));
    assert!(matches!(lister.picture, Shown::Failed("image_too_large")));
}
