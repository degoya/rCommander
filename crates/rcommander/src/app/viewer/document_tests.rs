//! A document's views and its place in the app (RC-100-10): the views it offers, the order `7`
//! steps through them, and the setting `plugins.viewer_token_colors` reaching every document.

use std::sync::Arc;

use rc_engine::{PanelId, VfsPath};

use super::{DocView, Document};
use crate::app::{
    harness::{FakeSource, Harness},
    viewer::{ViewerMessage, lister::Lister},
};

fn document(views: &[DocView]) -> Option<Document> {
    Document::new(views.to_vec(), Arc::new(FakeSource))
}

#[test]
fn a_document_offers_each_view_once_and_needs_one() {
    assert!(document(&[]).is_none());
    let views = [DocView::Highlighted, DocView::Tree, DocView::Highlighted];
    let mut shown = document(&views).expect("two views");
    assert_eq!(shown.views(), [DocView::Highlighted, DocView::Tree]);
    assert_eq!(shown.view(), DocView::Highlighted, "the first is shown");
    assert_eq!(shown.next_view(), DocView::Tree);
    assert_eq!(shown.next_view(), DocView::Highlighted, "round again");
    assert!(!shown.choose(DocView::Highlighted), "shown already");
    assert!(!shown.choose(DocView::Image), "not offered");
    assert!(shown.choose(DocView::Tree));
    shown.leave_tree();
    assert_eq!(shown.view(), DocView::Highlighted);

    let other = document(&[DocView::Text]).expect("one view");
    assert_ne!(
        other.serial(),
        shown.serial(),
        "an answer names its document"
    );
}

#[test]
fn the_tree_leaves_for_the_plain_text_without_a_coloured_one() {
    let mut shown = document(&[DocView::Tree, DocView::Image, DocView::Text]).expect("views");
    shown.leave_tree();
    assert_eq!(shown.view(), DocView::Text);
    let mut tree_only = document(&[DocView::Tree, DocView::Image]).expect("views");
    tree_only.leave_tree();
    assert_eq!(tree_only.view(), DocView::Image, "else the next view");
}

#[tokio::test]
async fn the_setting_reaches_every_document_and_the_next_one() {
    let root = tempfile::tempdir().expect("tempdir");
    let mut harness = Harness::headless(root.path()).await;
    let path = VfsPath::local(root.path().join("doc.json"));
    harness
        .app
        .viewers
        .open
        .push(Lister::new(7, PanelId::Left, path.clone()));
    let shown = document(&[DocView::Highlighted]).expect("a view");
    harness.app.attach_document(7, shown);
    let colors = |harness: &Harness, id: u64| {
        let lister = harness.app.viewers.lister(id).expect("open");
        lister.document.as_ref().expect("a document").colors
    };
    assert!(colors(&harness, 7), "on by default");

    harness.app.on_viewer(ViewerMessage::TokenColors(false));
    assert!(!harness.app.viewers.token_colors.0);
    assert!(!colors(&harness, 7));

    harness
        .app
        .viewers
        .open
        .push(Lister::new(8, PanelId::Left, path));
    let next = document(&[DocView::Highlighted, DocView::Tree]).expect("views");
    harness.app.attach_document(8, next);
    assert!(
        !colors(&harness, 8),
        "a document attached later starts as the setting says"
    );
}
