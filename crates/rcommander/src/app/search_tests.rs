//! The find-files dialog through `apply`, against a real engine: Alt+F7 opens it with the
//! keyboard, a search's batches fill the hit list, "Go to file" and "Feed to listbox" move the
//! panel. The iced task that streams a search does not run in the harness; the tests hand the
//! dialog what the engine's streaming search reports, as that task does.

use std::{
    path::{MAIN_SEPARATOR, Path, PathBuf},
    time::Duration,
};

use rc_engine::{
    EntryKind, FlatSource, MAX_FLAT_ENTRIES, PanelId, PanelState, SearchCancel, SearchEvent,
    SearchHit, VfsPath,
};
use rc_vfs::ArchiveFormat;
use tempfile::TempDir;

use super::{Everything, Flag, Run, SearchMsg, Setup, Update};
use crate::app::{
    Message,
    harness::{self, Harness},
};

/// `root/` with `a.txt`, `sub/c.txt` and `sub/d.log`.
fn tree() -> TempDir {
    harness::tree(&[("a.txt", "x"), ("sub/c.txt", "x"), ("sub/d.log", "x")])
}

async fn send(harness: &mut Harness, message: SearchMsg) {
    harness.send(Message::Search(message)).await;
}

/// Runs the dialog's search on the engine and hands it every event, as the stream does.
async fn search(harness: &mut Harness, name: &str) {
    send(
        harness,
        SearchMsg::Text(super::Field::Name, name.to_owned()),
    )
    .await;
    send(harness, SearchMsg::Start).await;
    let dialog = harness.app.search.as_ref().expect("open");
    assert!(dialog.running());
    let generation = dialog.generation;
    let query = dialog.form.query().expect("a query");
    let mut handle = harness
        .app
        .engine()
        .start_search(Some(dialog.panel), &dialog.form.start_folders(), &query)
        .await
        .expect("search");
    let mut updates = vec![Update::Started(handle.canceller())];
    while let Some(event) = handle.next().await {
        updates.push(Update::Event(Box::new(event)));
    }
    send(harness, SearchMsg::Updates(generation, updates)).await;
}

fn hit_names(harness: &Harness) -> Vec<String> {
    let dialog = harness.app.search.as_ref().expect("open");
    let mut names: Vec<String> = dialog
        .results
        .hits()
        .iter()
        .map(|hit| hit.entry.name.clone())
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn alt_f7_opens_the_dialog_on_the_panels_folder_and_takes_the_keys() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Alt+F7").await;
    let dialog = harness.app.search.as_ref().expect("the dialog");
    assert!(dialog.open);
    assert_eq!(dialog.panel, PanelId::Left);
    assert_eq!(dialog.form.start, root.path().display().to_string());

    let before = harness.app.engine().panel(PanelId::Left).cursor;
    harness.press("ArrowDown").await;
    harness.press("Tab").await;
    assert_eq!(harness.app.engine().panel(PanelId::Left).cursor, before);
    assert_eq!(harness.app.engine().active_panel(), PanelId::Left);

    harness.press("Escape").await;
    assert!(!harness.app.search.as_ref().expect("kept").open);
    harness.press("Tab").await;
    assert_eq!(
        harness.app.engine().active_panel(),
        PanelId::Right,
        "keys reach the panels"
    );
}

#[tokio::test]
async fn hits_arrive_and_a_closed_dialog_keeps_them() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Alt+F7").await;
    search(&mut harness, "*.txt").await;
    assert_eq!(hit_names(&harness), ["a.txt", "c.txt"]);
    let dialog = harness.app.search.as_ref().expect("open");
    assert!(matches!(
        dialog.run,
        Run::Finished {
            cancelled: false,
            ..
        }
    ));

    // A late batch of an earlier search is dropped.
    let late = dialog.generation - 1;
    send(
        &mut harness,
        SearchMsg::Updates(late, vec![Update::Failed("not_found")]),
    )
    .await;
    assert!(matches!(
        harness.app.search.as_ref().expect("open").run,
        Run::Finished { .. }
    ));

    harness.press("Escape").await;
    harness.press("Alt+F7").await;
    assert_eq!(hit_names(&harness), ["a.txt", "c.txt"]);
}

#[tokio::test]
async fn a_field_that_does_not_read_keeps_the_search_from_starting() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Alt+F7").await;
    send(
        &mut harness,
        SearchMsg::Text(super::Field::MinSize, "big".into()),
    )
    .await;
    harness.press("Enter").await;
    let dialog = harness.app.search.as_ref().expect("open");
    assert_eq!(dialog.invalid, Some(super::Field::MinSize));
    assert_eq!(dialog.run, Run::Idle);
    send(
        &mut harness,
        SearchMsg::Text(super::Field::MinSize, "1k".into()),
    )
    .await;
    assert_eq!(harness.app.search.as_ref().expect("open").invalid, None);
}

#[tokio::test]
async fn go_to_file_shows_the_hits_folder_with_the_cursor_on_it() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Alt+F7").await;
    search(&mut harness, "*.log").await;
    send(&mut harness, SearchMsg::GoTo).await;
    let panel = harness.app.engine().panel(PanelId::Left);
    assert_eq!(panel.path, VfsPath::local(root.path().join("sub")));
    assert_eq!(
        panel.cursor_entry().map(|entry| entry.name.as_str()),
        Some("d.log")
    );
    assert!(!harness.app.search.as_ref().expect("kept").open);
}

#[tokio::test]
async fn feed_to_listbox_shows_every_hit_flat_in_the_panel() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Alt+F7").await;
    search(&mut harness, "*.txt").await;
    send(&mut harness, SearchMsg::Feed).await;
    let mut panel = harness.app.engine().panel(PanelId::Left);
    for _ in 0..500 {
        if panel.flat.as_ref().is_some_and(|flat| !flat.loading) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        panel = harness.app.engine().panel(PanelId::Left);
    }
    assert_eq!(panel.path, VfsPath::local(root.path()));
    assert_eq!(panel.flat.expect("flat").source, FlatSource::Paths);
    let names: Vec<&str> = panel
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(names, ["a.txt", &format!("sub{MAIN_SEPARATOR}c.txt")]);
}

#[tokio::test]
async fn a_full_hit_list_stops_the_search() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Alt+F7").await;
    send(&mut harness, SearchMsg::Start).await;
    let generation = harness.app.search.as_ref().expect("open").generation;
    let cancel = SearchCancel::default();
    let hit = SearchHit {
        path: VfsPath::local(root.path().join("a.txt")),
        entry: rc_vfs::Entry {
            name: "a.txt".into(),
            kind: EntryKind::File,
            size: 1,
            modified: None,
            created: None,
            accessed: None,
            attributes: rc_vfs::Attributes::default(),
            hidden: false,
            owner: None,
        },
    };
    let mut updates = vec![Update::Started(cancel.clone())];
    updates.extend(
        (0..=MAX_FLAT_ENTRIES).map(|_| Update::Event(Box::new(SearchEvent::Hit(hit.clone())))),
    );
    send(&mut harness, SearchMsg::Updates(generation, updates)).await;
    assert!(cancel.is_cancelled());
    let dialog = harness.app.search.as_ref().expect("open");
    assert_eq!(dialog.results.len(), MAX_FLAT_ENTRIES);
    assert!(dialog.results.truncated);
}

/// An archive fixture of rc-vfs (`tests/fixtures/archives/`).
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../rc-vfs/tests/fixtures/archives")
        .join(name)
}

/// [`tree`] with copies of the archive fixtures `names`.
fn with_archives(names: &[&str]) -> TempDir {
    let root = tree();
    for name in names {
        std::fs::copy(fixture(name), root.path().join(name)).expect("copy the fixture");
    }
    root
}

/// The dialog's search of `name` with "Search archives" ticked.
async fn search_archives(harness: &mut Harness, name: &str) {
    send(harness, SearchMsg::Flag(Flag::Archives, true)).await;
    search(harness, name).await;
}

/// The left panel once its flat view is filled.
async fn fed(harness: &Harness) -> PanelState {
    let mut panel = harness.app.engine().panel(PanelId::Left);
    for _ in 0..500 {
        if panel.flat.as_ref().is_some_and(|flat| !flat.loading) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        panel = harness.app.engine().panel(PanelId::Left);
    }
    panel
}

fn names(panel: &PanelState) -> Vec<String> {
    let mut names: Vec<String> = panel
        .entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    names.sort();
    names
}

/// "Go to file" on a hit inside an archive opens the archive at the hit's folder.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn go_to_file_opens_the_archive_at_the_hit() {
    let root = with_archives(&["tree.zip"]);
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Alt+F7").await;
    search_archives(&mut harness, "readme.md").await;
    assert_eq!(hit_names(&harness), ["readme.md"]);
    send(&mut harness, SearchMsg::GoTo).await;
    let panel = harness.app.engine().panel(PanelId::Left);
    let archive = VfsPath::local(root.path().join("tree.zip"));
    assert_eq!(
        panel.path,
        VfsPath::archive(archive, ArchiveFormat::Zip, "/docs")
    );
    assert_eq!(
        panel.cursor_entry().map(|entry| entry.name.as_str()),
        Some("readme.md")
    );
}

/// All hits in one archive: "Feed to listbox" shows them in the archive.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn feed_hits_from_one_archive() {
    let root = with_archives(&["tree.zip"]);
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Alt+F7").await;
    search_archives(&mut harness, "readme.md;hello.txt").await;
    send(&mut harness, SearchMsg::Feed).await;
    let panel = fed(&harness).await;
    assert_eq!(
        rc_engine::archive_file(&panel.path),
        Some(&VfsPath::local(root.path().join("tree.zip")))
    );
    assert_eq!(
        names(&panel),
        [
            format!("docs{MAIN_SEPARATOR}readme.md"),
            "hello.txt".to_owned()
        ]
    );
}

/// Hits in two archives: "Feed to listbox" shows the archives themselves, each once, beside the
/// other hits.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn feed_lists_the_archives_themselves() {
    let root = with_archives(&["tree.7z", "tree.zip"]);
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Alt+F7").await;
    search_archives(&mut harness, "hello.txt;a.txt").await;
    send(&mut harness, SearchMsg::Feed).await;
    let panel = fed(&harness).await;
    assert_eq!(panel.path, VfsPath::local(root.path()));
    assert_eq!(names(&panel), ["a.txt", "tree.7z", "tree.zip"]);
}

/// The settings reach the form at the first opening only; Everything is used only while it runs,
/// asked by its box or by `ev:`.
#[tokio::test]
async fn the_setup_ticks_search_archives_once_and_finds_everything() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Alt+F7").await;
    let setup = |archives, everything| {
        SearchMsg::Setup(Setup {
            archives,
            everything,
        })
    };
    send(&mut harness, setup(true, Everything::Missing)).await;
    send(
        &mut harness,
        SearchMsg::Text(super::Field::Name, "ev:*.txt".into()),
    )
    .await;
    let dialog = harness.app.search.as_ref().expect("open");
    assert!(dialog.form.archives);
    assert!(!dialog.uses_everything(), "not while it does not run");
    send(&mut harness, setup(false, Everything::Ready)).await;
    let dialog = harness.app.search.as_ref().expect("open");
    assert!(dialog.form.archives, "the default is the first opening's");
    assert!(dialog.uses_everything());
}

/// A panel inside an archive (or on a server) is no local file system: the box stays off there.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn archives_are_searched_from_local_panels_only() {
    let root = with_archives(&["tree.zip"]);
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Alt+F7").await;
    assert!(harness.app.search.as_ref().expect("open").local);
    harness.press("Escape").await;
    harness
        .app
        .engine()
        .dispatch(rc_engine::Command::Navigate {
            panel: Some(PanelId::Left),
            path: root.path().join("tree.zip").display().to_string(),
        })
        .await
        .expect("into the archive");
    harness.press("Alt+F7").await;
    assert!(!harness.app.search.as_ref().expect("open").local);
}

/// "Find duplicate files" (RC-100-98): the groups arrive after the walk with a line each, and
/// "Feed to listbox" hands the panel each file's group.
#[tokio::test]
async fn duplicate_groups_show_with_their_lines_and_feed_with_their_groups() {
    let root = harness::tree(&[("a.txt", "same"), ("sub/a.txt", "same"), ("b.txt", "diff")]);
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Alt+F7").await;
    send(&mut harness, SearchMsg::Flag(Flag::Duplicates, true)).await;
    send(&mut harness, SearchMsg::Flag(Flag::SameName, false)).await;
    send(&mut harness, SearchMsg::Flag(Flag::SameContents, true)).await;
    search(&mut harness, "*.*").await;
    let dialog = harness.app.search.as_ref().expect("open");
    assert!(dialog.results.is_duplicates());
    assert_eq!(dialog.results.group_numbers(), [0, 0]);
    assert_eq!(dialog.results.row_count(), 3, "a line and two files");
    send(&mut harness, SearchMsg::Feed).await;
    let mut panel = harness.app.engine().panel(PanelId::Left);
    for _ in 0..500 {
        if panel.flat.as_ref().is_some_and(|flat| !flat.loading) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        panel = harness.app.engine().panel(PanelId::Left);
    }
    assert_eq!(panel.flat.expect("flat").source, FlatSource::Duplicates);
    assert_eq!(panel.entries.len(), 2);
}
