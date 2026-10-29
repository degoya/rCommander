//! A document's picture and text in one Lister (RC-100-29), through the Lister's `update()`: `7`
//! steps between the picture and the text either way round, the title row lists both, the
//! output not written yet is asked for once and only on the first switch to it, and a second
//! output that fails leaves the first view working with a notice. The source is a fake; its
//! answers are handed over as the window hands them.

use std::sync::Arc;

use iced::widget::image::Handle;
use rc_engine::{PanelId, VfsPath, viewer::ViewMode};
use tempfile::TempDir;

use crate::{
    app::{
        harness::{FakeSource, headless_engine},
        viewer::{
            document::{DocView, Document, DocumentMessage, TextFile},
            keys::ListerKey,
            lister::{Lister, Request, Shown},
            load::{self, Opened, Picture},
        },
    },
    i18n,
};

/// The file itself and the text its plugin wrote, opened as the window opens them.
struct Files {
    _root: TempDir,
    path: VfsPath,
    original: Opened,
    text: Opened,
}

const SOURCE: &[u8] = b"<svg>\n  <rect width=\"2\"/>\n</svg>\n";

async fn files() -> Files {
    let root = tempfile::tempdir().expect("tempdir");
    let path = VfsPath::local(root.path().join("logo.svg"));
    std::fs::write(&path.path, b"<svg/>").expect("write");
    let written = VfsPath::local(root.path().join("rcommander-view-text"));
    std::fs::write(&written.path, SOURCE).expect("write");
    let engine = headless_engine(root.path()).await;
    let original = load::open(engine.clone(), path.clone(), None)
        .await
        .expect("opens");
    let mut text = load::open(engine, written, None).await.expect("opens");
    text.mode = ViewMode::Text;
    Files {
        _root: root,
        path,
        original,
        text,
    }
}

fn picture() -> Picture {
    Picture {
        handle: Handle::from_rgba(2, 1, vec![0; 8]),
        width: 2,
        height: 1,
        format: "RGBA",
    }
}

/// A lister on `opened` showing a document of `views`; the requests it made.
fn showing(
    files: &Files,
    opened: Opened,
    views: &[DocView],
    later: bool,
) -> (Lister, Vec<Request>) {
    let mut lister = Lister::new(1, PanelId::Left, files.path.clone());
    let mut requests = lister.opened(opened);
    let mut document = Document::new(views.to_vec(), Arc::new(FakeSource)).expect("views");
    if later {
        document = document.text_later();
    }
    requests.extend(lister.set_document(document));
    (lister, requests)
}

fn document(lister: &Lister) -> &Document {
    lister.document.as_ref().expect("a document")
}

fn asks_picture(requests: &[Request]) -> bool {
    requests
        .iter()
        .any(|request| matches!(request, Request::Picture { .. }))
}

fn asks_text(requests: &[Request]) -> bool {
    requests
        .iter()
        .any(|request| matches!(request, Request::Text))
}

fn reads_page(requests: &[Request]) -> bool {
    requests
        .iter()
        .any(|request| matches!(request, Request::Page { .. }))
}

/// The SVG viewer's order: the picture first, the source written on the first `7`.
#[tokio::test]
async fn seven_steps_from_the_picture_to_the_text_written_on_the_first_switch() {
    let files = files().await;
    let views = [DocView::Image, DocView::Highlighted];
    let (mut lister, requests) = showing(&files, files.original.clone(), &views, true);
    assert_eq!(lister.mode, ViewMode::Image, "the picture first");
    assert!(asks_picture(&requests));
    assert!(!asks_text(&requests), "the text waits for the switch");
    assert_eq!(document(&lister).views(), views, "the title row lists both");
    let serial = document(&lister).serial();
    assert!(
        lister
            .document_message(DocumentMessage::Picture(serial, Ok(picture())))
            .is_empty()
    );
    assert!(matches!(lister.picture, Shown::Ready(_)));

    let seven = lister.key(ListerKey::NextView);
    assert!(matches!(seven.as_slice(), [Request::Text]));
    assert_eq!(document(&lister).view(), DocView::Highlighted);
    assert_eq!(
        lister.mode,
        ViewMode::Image,
        "the picture until the text is there"
    );
    assert!(
        lister.key(ListerKey::NextView).is_empty(),
        "back to the picture"
    );
    assert!(
        lister.key(ListerKey::NextView).is_empty(),
        "the text is asked for once"
    );
    assert_eq!(document(&lister).text_file(), TextFile::Asked);

    let arrived = lister.document_message(DocumentMessage::Text(serial, Ok(files.text.clone())));
    assert!(reads_page(&arrived));
    assert_eq!(lister.mode, ViewMode::Text);
    assert_eq!(
        lister.size(),
        SOURCE.len() as u64,
        "the text is the file now"
    );
    assert_eq!(document(&lister).text_file(), TextFile::Shown);
    assert!(lister.note.is_none());

    let back = lister.key(ListerKey::NextView);
    assert_eq!(lister.mode, ViewMode::Image);
    assert!(back.is_empty(), "the picture is kept, not read again");
    let text = lister.key(ListerKey::NextView);
    assert_eq!(lister.mode, ViewMode::Text);
    assert!(reads_page(&text) && !asks_text(&text), "written once");
}

/// The other way round: the text first, the picture read on the first switch to it.
#[tokio::test]
async fn seven_steps_from_the_text_to_the_picture_read_on_the_first_switch() {
    let files = files().await;
    let views = [DocView::Highlighted, DocView::Image];
    let (mut lister, requests) = showing(&files, files.text.clone(), &views, false);
    assert_eq!(lister.mode, ViewMode::Text);
    assert!(reads_page(&requests));
    assert!(!asks_picture(&requests), "the picture waits for the switch");

    let seven = lister.key(ListerKey::NextView);
    assert!(matches!(seven.as_slice(), [Request::Picture { .. }]));
    assert_eq!(lister.mode, ViewMode::Image);
    let serial = document(&lister).serial();
    lister.document_message(DocumentMessage::Picture(serial, Ok(picture())));
    assert!(matches!(lister.picture, Shown::Ready(_)));

    let chosen = lister.document_message(DocumentMessage::View(DocView::Highlighted));
    assert_eq!(lister.mode, ViewMode::Text, "the list in the title row too");
    assert!(reads_page(&chosen));
    assert!(
        !asks_picture(&lister.key(ListerKey::NextView)),
        "the picture is kept"
    );
    assert_eq!(lister.mode, ViewMode::Image);
}

/// A text that cannot be written: the notice, the text views gone, the picture still shown.
#[tokio::test]
async fn a_failing_second_text_leaves_the_picture_with_a_notice() {
    let files = files().await;
    let views = [DocView::Image, DocView::Highlighted];
    let (mut lister, _) = showing(&files, files.original.clone(), &views, true);
    let serial = document(&lister).serial();
    lister.document_message(DocumentMessage::Picture(serial, Ok(picture())));
    assert!(asks_text(&lister.key(ListerKey::NextView)));

    let stale = lister.document_message(DocumentMessage::Text(serial + 1, Err("plugin_crashed")));
    assert!(stale.is_empty(), "another document's answer");
    assert_eq!(document(&lister).views(), views);

    lister.document_message(DocumentMessage::Text(serial, Err("plugin_crashed")));
    assert_eq!(document(&lister).views(), [DocView::Image]);
    assert_eq!(document(&lister).view(), DocView::Image);
    assert_eq!(lister.mode, ViewMode::Image);
    assert!(
        matches!(lister.picture, Shown::Ready(_)),
        "the first view works"
    );
    assert_eq!(
        lister.note.as_deref(),
        Some(i18n::error_text("plugin_crashed").as_str())
    );
}

/// A picture that cannot be written beside the text: the notice and the text again; one that is
/// the document's first output fails in the image view, as a file's picture does.
#[tokio::test]
async fn a_failing_picture_is_a_notice_beside_the_text_and_a_message_on_its_own() {
    let files = files().await;
    let (mut lister, _) = showing(
        &files,
        files.text.clone(),
        &[DocView::Highlighted, DocView::Image],
        false,
    );
    let serial = document(&lister).serial();
    assert!(asks_picture(&lister.key(ListerKey::NextView)));
    let back = lister.document_message(DocumentMessage::Picture(serial, Err("plugin_timed_out")));
    assert_eq!(document(&lister).views(), [DocView::Highlighted]);
    assert_eq!(lister.mode, ViewMode::Text);
    assert!(reads_page(&back));
    assert_eq!(
        lister.note.as_deref(),
        Some(i18n::error_text("plugin_timed_out").as_str())
    );

    let views = [DocView::Image, DocView::Highlighted];
    let (mut first, _) = showing(&files, files.original.clone(), &views, true);
    let serial = document(&first).serial();
    first.document_message(DocumentMessage::Picture(serial, Err("image_broken")));
    assert!(matches!(first.picture, Shown::Failed("image_broken")));
    assert_eq!(document(&first).views(), views, "the text is still offered");
}
