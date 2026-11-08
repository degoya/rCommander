//! Certificates and keys in the built-in viewer (RC-110-08): a key file opens as its report,
//! the text in place of the file and the tree on `7`; `viewer.certificates` off or another file
//! opens as before.

use std::{fs, path::Path};

use rc_db::Db;
use rc_engine::{PanelId, VfsPath};

use super::{FIELD_IDS, document, open};
use crate::app::{
    harness::Harness,
    viewer::{ViewerMessage, document::DocView, lister::Lister, load, plugin::PluginOpened},
};

const CHAIN_AND_KEY: &[u8] = include_bytes!("../../../../rc-cert/tests/fixtures/chain-and-key.pem");

async fn opened(
    dir: &std::path::Path,
    name: &str,
    bytes: &[u8],
    harness: &Harness,
) -> load::Opened {
    let path = dir.join(name);
    fs::write(&path, bytes).expect("write");
    load::open(harness.app.engine().clone(), VfsPath::local(path), None)
        .await
        .expect("open")
}

#[tokio::test]
async fn a_key_file_opens_as_its_report_and_tree() {
    let root = tempfile::tempdir().expect("temp dir");
    let harness = Harness::headless(root.path()).await;
    let file = opened(root.path(), "server.pem", CHAIN_AND_KEY, &harness).await;
    let (report, source) = open(Path::new("server.pem"), &file, None)
        .await
        .expect("a report");
    let size = usize::try_from(report.file.size()).expect("small");
    let text = report
        .file
        .read_uncached(0, size)
        .await
        .expect("the report's text");
    let text = String::from_utf8(text).expect("utf-8");
    assert!(text.starts_with(crate::i18n::t!("viewer.cert.checks")));
    assert!(!text.contains("BEGIN"));
    let roots = source.nodes(None);
    assert_eq!(roots.len(), 4, "the checks and three objects");
    assert!(roots.iter().all(|node| node.has_children));
    let fields = source.nodes(Some(roots.get(3).expect("the key").id));
    assert!(
        fields
            .iter()
            .all(|node| node.id >= FIELD_IDS && !node.has_children)
    );
    assert!(source.nodes(Some(FIELD_IDS)).is_empty());
    let shown = document(source).expect("a document");
    assert_eq!(shown.views(), [DocView::Text, DocView::Tree]);
}

#[tokio::test]
async fn other_files_and_the_switch_off_open_as_before() {
    let root = tempfile::tempdir().expect("temp dir");
    let harness = Harness::headless(root.path()).await;
    let notes = opened(root.path(), "notes.txt", b"hello\n", &harness).await;
    assert!(open(Path::new("notes.txt"), &notes, None).await.is_none());
    let key = opened(root.path(), "server.pem", CHAIN_AND_KEY, &harness).await;
    let db = Db::open_in_memory().await.expect("db");
    db.set(rc_engine::settings::VIEWER_CERTIFICATES_KEY, &false)
        .await
        .expect("set");
    assert!(
        open(Path::new("server.pem"), &key, Some(&db))
            .await
            .is_none()
    );
}

/// The lister takes the report: its text shown first, `7` steps to the tree.
#[tokio::test]
async fn the_lister_shows_the_report_and_seven_steps_to_the_tree() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::headless(root.path()).await;
    let file = opened(root.path(), "server.pem", CHAIN_AND_KEY, &harness).await;
    let (report, source) = open(Path::new("server.pem"), &file, None)
        .await
        .expect("a report");
    let path = VfsPath::local(root.path().join("server.pem"));
    harness
        .app
        .viewers
        .open
        .push(Lister::new(7, PanelId::Left, path));
    let effects = harness.app.on_viewer(ViewerMessage::Plugin(
        7,
        PluginOpened::Certificates {
            opened: report,
            source,
        },
    ));
    assert!(!effects.is_empty(), "the text is read");
    let lister = harness.app.viewers.lister_mut(7).expect("open");
    let document = lister.document.as_ref().expect("a document");
    assert_eq!(document.view(), DocView::Text);
    let requests = lister.document_key(super::super::keys::ListerKey::NextView);
    assert!(lister.tree_shown());
    assert!(requests.is_some_and(|requests| !requests.is_empty()));
}

/// The report is shown from memory: its text has no file on any disk, and opening it adds no
/// file beside the original.
#[tokio::test]
async fn no_file_is_created_for_a_report() {
    let root = tempfile::tempdir().expect("temp dir");
    let harness = Harness::headless(root.path()).await;
    let file = opened(root.path(), "server.pem", CHAIN_AND_KEY, &harness).await;
    let before: Vec<_> = fs::read_dir(root.path()).expect("list").collect();
    let (report, _) = open(Path::new("server.pem"), &file, None)
        .await
        .expect("a report");
    let after: Vec<_> = fs::read_dir(root.path()).expect("list").collect();
    assert_eq!(before.len(), after.len());
    assert_eq!(report.file.path(), Path::new(super::REPORT_NAME));
    assert!(
        !report.file.path().exists(),
        "the report's text is on no disk"
    );
    assert!(report.file.size() > 0);
}
