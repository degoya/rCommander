//! A plugin document's views through the Lister's `update()` (RC-100-10): switching views, the
//! tokens of the lines on screen and nothing else, invalid spans, the same text with and without
//! colour, the colours off, and the tree by key. The source is a fake; the answers are handed
//! over as the window would hand them.

use std::{ops::Range, sync::Arc};

use iced::widget::text_editor::Action;
use rc_engine::{PanelId, VfsPath, viewer::ViewMode};
use tempfile::TempDir;

use crate::app::harness::{FakeSource, headless_engine};
use crate::app::viewer::{
    document::{DocView, Document, DocumentMessage, Token, TokenClass, TreeNode},
    keys::{ListerKey, Scroll},
    lister::{Lister, Request},
    load,
};

fn document(views: &[DocView]) -> Document {
    Document::new(views.to_vec(), Arc::new(FakeSource)).expect("a view")
}

/// A lister of `rows` rows on a file holding `bytes`, showing a document of `views` (none for a
/// plain file) once its first page is in; the requests left over.
async fn shown(
    bytes: &[u8],
    views: Option<&[DocView]>,
    rows: usize,
) -> (TempDir, Lister, Vec<Request>) {
    let root = tempfile::tempdir().expect("tempdir");
    let path = VfsPath::local(root.path().join("doc.json"));
    std::fs::write(&path.path, bytes).expect("write");
    let engine = headless_engine(root.path()).await;
    let opened = load::open(engine, path.clone(), None).await.expect("opens");
    let mut lister = Lister::new(1, PanelId::Left, path);
    lister.rows = rows;
    let mut requests = lister.opened(opened);
    if let Some(views) = views {
        requests.extend(lister.set_document(document(views)));
    }
    let requests = deliver(&mut lister, requests).await;
    (root, lister, requests)
}

/// The page reads among `requests` run and answered, as the window runs them; the rest.
async fn deliver(lister: &mut Lister, requests: Vec<Request>) -> Vec<Request> {
    let mut rest = Vec::new();
    for request in requests {
        match request {
            Request::Page {
                generation,
                seek,
                mark,
            } => {
                let opened = lister.file.as_ref().expect("open");
                let file = (opened.file.clone(), opened.last_page.clone());
                let page = load::page(file, lister.layout(), lister.top, seek, mark).await;
                rest.extend(lister.paged(generation, page));
            }
            other => rest.push(other),
        }
    }
    rest
}

fn serial(lister: &Lister) -> u64 {
    lister.document.as_ref().expect("a document").serial()
}

/// The spans painted on line `line` of the page.
fn spans_of(lister: &Lister, line: usize) -> Vec<(Range<usize>, TokenClass)> {
    let document = lister.document.as_ref().expect("a document");
    document.spans.shown().line(line).to_vec()
}

fn token(start: u32, len: u32, class: TokenClass) -> Token {
    Token { start, len, class }
}

fn asked(requests: &[Request]) -> Vec<u64> {
    match requests {
        [Request::Tokens(starts)] => starts.clone(),
        other => panic!("one question for tokens: {other:?}"),
    }
}

#[tokio::test]
async fn the_first_view_shows_and_seven_or_the_list_switches() {
    let views = [DocView::Highlighted, DocView::Tree];
    let (_root, mut lister, requests) = shown(b"{\"a\": 1}\n", Some(&views), 10).await;
    assert_eq!(asked(&requests).first(), Some(&0));
    assert!(!lister.tree_shown(), "the first view, the coloured text");

    let tree = lister.key(ListerKey::NextView);
    assert!(lister.tree_shown());
    assert!(matches!(tree.as_slice(), [Request::Children(None)]));
    let back = lister.key(ListerKey::NextView);
    assert!(back.is_empty(), "the page's tokens are asked already");
    assert!(!lister.tree_shown());

    let chosen = lister.document_message(DocumentMessage::View(DocView::Tree));
    assert!(chosen.is_empty(), "the top level is asked once");
    assert!(lister.tree_shown());
    let refused = lister.document_message(DocumentMessage::View(DocView::Markdown));
    assert!(refused.is_empty());
    assert!(lister.tree_shown(), "a view not offered is not shown");
}

#[tokio::test]
async fn only_the_lines_on_screen_are_asked_for_and_each_once() {
    let text: String = (0..500).map(|n| format!("line {n}\n")).collect();
    let (_root, mut lister, requests) =
        shown(text.as_bytes(), Some(&[DocView::Highlighted]), 10).await;
    let first = asked(&requests);
    assert_eq!(first.len(), 10, "the page, not the file");
    assert_eq!(first[..3], [0, 7, 14]);

    let down = lister.key(ListerKey::Scroll(Scroll::LineDown));
    let next = deliver(&mut lister, down).await;
    assert_eq!(asked(&next), [70], "only the line that came on screen");

    let serial = serial(&lister);
    let answer = vec![vec![token(0, 4, TokenClass::Keyword)]; first.len()];
    lister.document_message(DocumentMessage::Tokens(serial, first, Ok(answer)));
    assert_eq!(spans_of(&lister, 0), [(0..4, TokenClass::Keyword)]);
    let up = lister.key(ListerKey::Scroll(Scroll::LineUp));
    assert!(
        deliver(&mut lister, up).await.is_empty(),
        "lines answered are not asked again"
    );
}

#[tokio::test]
async fn spans_out_of_their_line_inside_a_character_or_for_another_document_are_ignored() {
    let (_root, mut lister, requests) =
        shown("k\u{e9}y: 1\n".as_bytes(), Some(&[DocView::Highlighted]), 5).await;
    let starts = asked(&requests);
    let serial = serial(&lister);
    let stale = vec![vec![token(0, 1, TokenClass::Keyword)]];
    lister.document_message(DocumentMessage::Tokens(
        serial + 1,
        starts.clone(),
        Ok(stale),
    ));
    assert!(spans_of(&lister, 0).is_empty(), "another document's answer");

    let line = vec![
        token(0, 4, TokenClass::Key),
        token(2, 1, TokenClass::Invalid),
        token(6, 1, TokenClass::Number),
        token(7, 5, TokenClass::String),
        token(3, 0, TokenClass::Comment),
        token(u32::MAX, u32::MAX, TokenClass::Keyword),
    ];
    let mut answer = vec![line];
    answer.extend(vec![
        vec![token(0, 1, TokenClass::Keyword)];
        starts.len() + 3
    ]);
    lister.document_message(DocumentMessage::Tokens(serial, starts, Ok(answer)));
    assert_eq!(
        spans_of(&lister, 0),
        [(0..4, TokenClass::Key), (6..7, TokenClass::Number)]
    );
}

#[tokio::test]
async fn text_with_spans_copies_and_searches_as_text_without() {
    let bytes = b"{\"name\": \"value\",\t\"n\": 42}\nsecond line\n";
    let (_a, mut plain, _) = shown(bytes, None, 5).await;
    let (_b, mut coloured, requests) = shown(bytes, Some(&[DocView::Highlighted]), 5).await;
    let starts = asked(&requests);
    let serial = serial(&coloured);
    let answer = vec![
        vec![
            token(1, 6, TokenClass::Key),
            token(9, 7, TokenClass::String),
            token(23, 2, TokenClass::Number),
        ];
        starts.len()
    ];
    coloured.document_message(DocumentMessage::Tokens(serial, starts, Ok(answer)));
    assert_eq!(spans_of(&coloured, 0).len(), 3, "the tab moves the number");
    // The tab before `"n"` stands for seven spaces at column 17: six bytes more.
    assert!(spans_of(&coloured, 0).contains(&(29..31, TokenClass::Number)));

    assert_eq!(coloured.content.text(), plain.content.text());
    plain.editor(Action::SelectAll);
    coloured.editor(Action::SelectAll);
    assert!(coloured.content.selection().is_some());
    assert_eq!(coloured.content.selection(), plain.content.selection());

    let search = |lister: &mut Lister| {
        lister.key(ListerKey::Search);
        lister.bar_input("value".to_owned());
        lister
            .submit()
            .into_iter()
            .find_map(|request| match request {
                Request::Search { needle, from, .. } => Some((needle, from)),
                _ => None,
            })
            .expect("a search")
    };
    assert_eq!(search(&mut coloured), search(&mut plain));
}

#[tokio::test]
async fn with_the_colours_off_or_in_hex_nothing_is_asked_or_coloured() {
    let root = tempfile::tempdir().expect("tempdir");
    let path = VfsPath::local(root.path().join("doc.json"));
    std::fs::write(&path.path, b"[1, 2]\n").expect("write");
    let engine = headless_engine(root.path()).await;
    let opened = load::open(engine, path.clone(), None).await.expect("opens");
    let mut lister = Lister::new(1, PanelId::Left, path);
    let mut requests = lister.opened(opened);
    let mut off = document(&[DocView::Highlighted]);
    off.colors = false;
    requests.extend(lister.set_document(off));
    assert!(deliver(&mut lister, requests).await.is_empty());
    assert!(spans_of(&lister, 0).is_empty());

    lister.document.as_mut().expect("a document").colors = true;
    let starts = asked(&lister.recolor());
    let serial = serial(&lister);
    let answer = vec![vec![token(1, 1, TokenClass::Number)]; starts.len()];
    lister.document_message(DocumentMessage::Tokens(serial, starts, Ok(answer)));
    assert_eq!(spans_of(&lister, 0), [(1..2, TokenClass::Number)]);

    lister.document.as_mut().expect("a document").colors = false;
    assert!(lister.recolor().is_empty());
    assert!(spans_of(&lister, 0).is_empty(), "off: no colours");

    lister.document.as_mut().expect("a document").colors = true;
    let hex = lister.key(ListerKey::Mode(ViewMode::Hex));
    assert!(deliver(&mut lister, hex).await.is_empty());
    assert!(spans_of(&lister, 0).is_empty(), "hex shows no colours");
}

fn ids(lister: &Lister) -> Vec<u64> {
    let tree = &lister.document.as_ref().expect("a document").tree;
    tree.visible().iter().map(|row| row.id).collect()
}

fn cursor_id(lister: &Lister) -> u64 {
    let tree = &lister.document.as_ref().expect("a document").tree;
    tree.visible()[tree.cursor - tree.top].id
}

fn node(id: u64, label: &str, has_children: bool) -> TreeNode {
    TreeNode {
        id,
        label: label.to_owned(),
        value: None,
        has_children,
    }
}

#[tokio::test]
async fn the_tree_unfolds_and_folds_by_key_and_loads_children_once() {
    let views = [DocView::Tree, DocView::Highlighted];
    let (_root, mut lister, requests) = shown(b"{}\n", Some(&views), 5).await;
    assert!(lister.tree_shown());
    assert!(matches!(requests.as_slice(), [Request::Children(None)]));
    let serial = serial(&lister);
    let top = vec![node(1, "a", true), node(2, "b", false)];
    lister.document_message(DocumentMessage::Children(serial, None, Ok(top)));
    assert_eq!(ids(&lister), [1, 2]);

    let right = lister.key(ListerKey::Scroll(Scroll::Right));
    assert!(matches!(right.as_slice(), [Request::Children(Some(1))]));
    let kids = vec![node(3, "c", false), node(1, "a again", true)];
    lister.document_message(DocumentMessage::Children(serial, Some(1), Ok(kids)));
    assert_eq!(ids(&lister), [1, 3, 2], "an id seen before is dropped");

    assert!(lister.key(ListerKey::Scroll(Scroll::Left)).is_empty());
    assert_eq!(ids(&lister), [1, 2], "folded");
    assert!(
        lister.key(ListerKey::Scroll(Scroll::Right)).is_empty(),
        "the children are loaded once"
    );
    assert_eq!(ids(&lister), [1, 3, 2]);
    lister.key(ListerKey::Scroll(Scroll::Right));
    assert_eq!(
        cursor_id(&lister),
        3,
        "Right on an unfolded node steps to its child"
    );
    lister.key(ListerKey::Scroll(Scroll::Left));
    assert_eq!(cursor_id(&lister), 1, "Left on a leaf steps to its parent");

    assert!(
        lister
            .document_message(DocumentMessage::TreeClick(0))
            .is_empty()
    );
    assert_eq!(ids(&lister), [1, 2], "a click folds");

    lister.key(ListerKey::Search);
    assert!(!lister.tree_shown(), "a search goes back to the text");
    assert!(lister.find.bar.is_some());
}
