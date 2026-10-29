//! The find-files dialog through `apply`, against a real engine: Alt+F7 opens it with the
//! keyboard, a search's batches fill the hit list, "Go to file" and "Feed to listbox" move the
//! panel. The iced task that streams a search does not run in the harness; the tests hand the
//! dialog what the engine's streaming search reports, as that task does.

use std::{path::MAIN_SEPARATOR, time::Duration};

use rc_engine::{
    EntryKind, FlatSource, MAX_FLAT_ENTRIES, PanelId, SearchCancel, SearchEvent, SearchHit, VfsPath,
};
use tempfile::TempDir;

use super::{Run, SearchMsg, Update};
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
        updates.push(Update::Event(event));
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
            attributes: rc_vfs::Attributes::default(),
            hidden: false,
            owner: None,
        },
    };
    let mut updates = vec![Update::Started(cancel.clone())];
    updates.extend((0..=MAX_FLAT_ENTRIES).map(|_| Update::Event(SearchEvent::Hit(hit.clone()))));
    send(&mut harness, SearchMsg::Updates(generation, updates)).await;
    assert!(cancel.is_cancelled());
    let dialog = harness.app.search.as_ref().expect("open");
    assert_eq!(dialog.results.len(), MAX_FLAT_ENTRIES);
    assert!(dialog.results.truncated);
}
