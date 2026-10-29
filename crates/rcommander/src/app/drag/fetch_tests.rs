use std::fs;

use super::*;
use crate::app::harness::headless_engine;

/// The left panel's state over a folder with `a.txt` (3 bytes), `b.txt` (5 bytes) and `sub/`.
async fn state() -> (tempfile::TempDir, PanelState) {
    let root = tempfile::tempdir().expect("temp dir");
    fs::write(root.path().join("a.txt"), b"aaa").expect("write");
    fs::write(root.path().join("b.txt"), b"bbbbb").expect("write");
    fs::create_dir(root.path().join("sub")).expect("mkdir");
    let engine = headless_engine(root.path()).await;
    let state = engine.panel(PanelId::Left);
    (root, state)
}

fn names(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|text| (*text).to_owned()).collect()
}

#[tokio::test]
async fn files_are_counted_and_a_small_fetch_asks_nothing() {
    let (_root, state) = state().await;
    let fetch = Fetch::new(
        PanelId::Left,
        &state,
        names(&["a.txt", "b.txt"]),
        Purpose::Drag,
    );
    assert_eq!(fetch.size, Some(8));
    assert!(!fetch.asks());
    assert_eq!(fetch.folder, None);
}

#[tokio::test]
async fn an_uncounted_folder_asks() {
    let (_root, state) = state().await;
    let fetch = Fetch::new(
        PanelId::Left,
        &state,
        names(&["a.txt", "sub"]),
        Purpose::Clipboard,
    );
    assert_eq!(fetch.size, None);
    assert!(fetch.asks());
}

#[tokio::test]
async fn the_same_listing_gives_the_same_key() {
    let (_root, state) = state().await;
    let first = Fetch::new(PanelId::Left, &state, names(&["a.txt"]), Purpose::Drag);
    let again = Fetch::new(
        PanelId::Right,
        &state,
        names(&["a.txt"]),
        Purpose::Clipboard,
    );
    assert_eq!(first.key, again.key);
    let other = Fetch::new(PanelId::Left, &state, names(&["b.txt"]), Purpose::Drag);
    assert_ne!(first.key, other.key);
}

#[test]
fn a_large_fetch_asks() {
    let mut fetch = Fetch {
        panel: PanelId::Left,
        names: Vec::new(),
        purpose: Purpose::Drag,
        key: FetchKey {
            folder: VfsPath::local("/"),
            entries: Vec::new(),
        },
        size: Some(ASK_ABOVE),
        folder: None,
        spoiled: false,
    };
    assert!(!fetch.asks());
    fetch.size = Some(ASK_ABOVE + 1);
    assert!(fetch.asks());
}

#[test]
fn only_a_local_other_panel_takes_a_fetch() {
    let ftp = VfsPath {
        backend: BackendId::Ftp { connection_id: 7 },
        path: PathBuf::from("/pub"),
    };
    assert!(remote(&ftp));
    assert!(!remote(&VfsPath::local("/data")));
    assert!(possible(&VfsPath::local("/data")));
    assert!(!possible(&ftp));
}

#[test]
fn the_fetch_copies_the_names_into_the_folder() {
    let command = command(PanelId::Right, &names(&["a.txt"]), Path::new("/tmp/x/1"));
    assert_eq!(
        command,
        Command::OpsCopy(TransferRequest {
            panel: Some(PanelId::Right),
            sources: names(&["a.txt"]),
            target: Some(Path::new("/tmp/x/1").display().to_string()),
            options: JobOptions::default(),
        })
    );
}

#[test]
fn fetched_copies_are_named_by_their_last_part_once() {
    let paths = fetched_paths(
        Path::new("/tmp/x/1"),
        &names(&["a.txt", "sub/b.txt", "other/b.txt", "dir"]),
    );
    assert_eq!(
        paths,
        vec![
            PathBuf::from("/tmp/x/1/a.txt"),
            PathBuf::from("/tmp/x/1/b.txt"),
            PathBuf::from("/tmp/x/1/dir"),
        ]
    );
}
