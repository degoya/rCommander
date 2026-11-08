//! Ctrl+A and Ctrl+C over the whole text, not the page (RC-110-06).

use std::sync::Arc;

use iced::widget::text_editor::{Action, Motion};
use rc_engine::{
    PanelId, VfsPath,
    viewer::{FileWindow, TextEncoding, ViewMode},
};
use tempfile::TempDir;

use super::{ALL_CODES, COPY_LIMIT, whole_text};
use crate::{
    app::{
        harness::headless_engine,
        viewer::{
            keys::{ListerKey, Scroll},
            lister::{Lister, Request},
            load::{self, Page},
            markdown::render,
        },
    },
    i18n,
};

/// A lister on the file `name` holding `bytes`, opened, and the generation of its first page.
async fn lister_on(name: &str, bytes: &[u8]) -> (TempDir, Lister, u64) {
    let root = tempfile::tempdir().expect("tempdir");
    let path = VfsPath::local(root.path().join(name));
    std::fs::write(&path.path, bytes).expect("write");
    let engine = headless_engine(root.path()).await;
    let opened = load::open(engine, path.clone(), None).await.expect("opens");
    let mut lister = Lister::new(1, PanelId::Left, path);
    let requests = lister.opened(opened);
    (root, lister, generation(&requests))
}

fn generation(requests: &[Request]) -> u64 {
    requests
        .iter()
        .find_map(|request| match request {
            Request::Page { generation, .. } => Some(*generation),
            _ => None,
        })
        .expect("a page asked for")
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

fn file_of(lister: &Lister) -> Arc<FileWindow> {
    Arc::clone(&lister.file.as_ref().expect("open").file)
}

/// A file of `lines` numbered lines, far more than a page.
fn long_text(lines: usize) -> String {
    (1..=lines).map(|n| format!("line {n}\n")).collect()
}

#[tokio::test]
async fn ctrl_a_selects_the_whole_text_and_ctrl_c_copies_all_of_it_not_the_page() {
    let text = long_text(500);
    let (_root, mut lister, first) = lister_on("long.txt", text.as_bytes()).await;
    lister.paged(first, page(0, &["line 1", "line 2", "line 3"]));

    assert!(lister.key(ListerKey::SelectAll).is_empty());
    assert!(lister.whole_selected());
    assert_eq!(
        lister.content.selection().as_deref(),
        Some("line 1\nline 2\nline 3"),
        "the page shows all its text selected"
    );
    assert!(matches!(
        lister.key(ListerKey::Copy).as_slice(),
        [Request::CopyAll]
    ));
    let copied = whole_text(file_of(&lister), lister.detected).await;
    assert_eq!(
        copied.as_deref(),
        Ok(text.as_str()),
        "every line, not the page"
    );
}

#[tokio::test]
async fn the_whole_selection_survives_scrolling_and_a_click_ends_it() {
    let (_root, mut lister, first) = lister_on("f.txt", long_text(200).as_bytes()).await;
    lister.paged(first, page(0, &["line 1", "line 2"]));
    lister.key(ListerKey::SelectAll);
    let down = lister.key(ListerKey::Scroll(Scroll::PageDown));
    lister.paged(generation(&down), page(14, &["line 3", "line 4"]));
    assert!(lister.whole_selected());
    assert_eq!(
        lister.content.selection().as_deref(),
        Some("line 3\nline 4")
    );

    // A selection made in the page is the page's again: Ctrl+C copies just that.
    lister.editor(Action::Move(Motion::DocumentStart));
    lister.editor(Action::SelectLine);
    assert!(!lister.whole_selected());
    assert!(matches!(
        lister.key(ListerKey::Copy).as_slice(),
        [Request::Copy(line)] if line.starts_with("line 3")
    ));
    // The editor's own Ctrl+A (focused) takes the whole text as the key does.
    lister.editor(Action::SelectAll);
    assert!(lister.whole_selected());
}

#[tokio::test]
async fn the_hex_view_copies_its_page_and_nothing_selected_copies_nothing() {
    let (_root, mut lister, first) = lister_on("f.txt", b"abc\n").await;
    lister.paged(first, page(0, &["abc"]));
    assert!(lister.key(ListerKey::Copy).is_empty(), "nothing selected");
    let hex = lister.key(ListerKey::Mode(ViewMode::Hex));
    lister.paged(generation(&hex), page(0, &["00000000  61 62 63 0A"]));
    lister.key(ListerKey::SelectAll);
    assert!(!lister.whole_selected());
    assert!(matches!(
        lister.key(ListerKey::Copy).as_slice(),
        [Request::Copy(page)] if page.starts_with("00000000")
    ));
}

#[tokio::test]
async fn the_whole_text_is_decoded_without_its_byte_order_mark() {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in "gr\u{fc}n\nzwei\n".encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let (_root, lister, _) = lister_on("u.txt", &bytes).await;
    assert_eq!(lister.detected.encoding, TextEncoding::Utf16Le);
    let copied = whole_text(file_of(&lister), lister.detected).await;
    assert_eq!(copied.as_deref(), Ok("gr\u{fc}n\nzwei\n"));
}

#[tokio::test]
async fn a_file_over_the_limit_is_not_copied_and_says_why() {
    let size = usize::try_from(COPY_LIMIT).expect("fits") + 1;
    let (_root, mut lister, _) = lister_on("big.txt", &vec![b'a'; size]).await;
    let copied = whole_text(file_of(&lister), lister.detected).await;
    assert_eq!(copied, Err("viewer_copy_too_large"));
    assert!(lister.copied(copied).is_empty());
    assert_eq!(
        lister.note.as_deref(),
        Some(i18n::error_text("viewer_copy_too_large").as_str())
    );
    for code in ALL_CODES {
        assert_ne!(i18n::error_text(code), *code, "error.{code} is missing");
    }
}

#[tokio::test]
async fn in_the_markdown_preview_ctrl_a_and_ctrl_c_take_every_block() {
    let source: String = (1..=300)
        .map(|n| format!("Paragraph {n}\n\n"))
        .chain(["```sh\necho last\n```\n".to_owned()])
        .collect();
    let (_root, mut lister, _) = lister_on("long.md", source.as_bytes()).await;
    let rendered = render(file_of(&lister), lister.detected, None, true).await;
    let path = lister.path.clone();
    lister.rendered(&path, rendered);
    assert!(lister.selection().is_some(), "the preview selects");

    assert!(lister.key(ListerKey::SelectAll).is_empty());
    let copied = match lister.key(ListerKey::Copy).as_slice() {
        [Request::Copy(text)] => text.clone(),
        other => panic!("not copied: {other:?}"),
    };
    assert!(copied.starts_with("Paragraph 1\n"));
    assert!(copied.contains("Paragraph 300\n"));
    assert!(copied.ends_with("echo last"), "{copied:?}");
}
