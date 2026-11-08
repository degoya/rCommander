//! The name lists and the next step through the window's `update()` (RC-100-96), against a real
//! engine with a database. The editor itself is not started: its answer is sent as the call
//! would send it.

use std::{fs, path::Path, time::Duration};

use rc_db::Db;
use rc_engine::{Command, PanelId};
use tempfile::TempDir;

use super::{ClipboardUse, NamesMsg};
use crate::app::multi_rename::{Answer, Field, MrMsg, MultiRenameDialog};
use crate::app::{
    Message,
    harness::{Harness, headless_engine},
};

/// `root/` with `a.txt`, `b.txt` and `c.txt`, each holding its own name; the dialog open on the
/// selection `a.txt` and `c.txt`.
async fn open() -> (TempDir, Harness) {
    let root = tempfile::tempdir().expect("temp dir");
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(root.path().join(name), name).expect("write");
    }
    let engine = headless_engine(root.path()).await;
    let db = Db::open_in_memory().await.expect("db");
    assert!(engine.attach_db(db.clone()));
    let mut harness = Harness::new(engine);
    harness.app.db = Some(db);
    harness
        .app
        .engine()
        .dispatch(Command::ToggleSelection {
            panel: Some(PanelId::Left),
            names: vec!["c.txt".into(), "a.txt".into()],
        })
        .await
        .expect("selects");
    harness.settle().await;
    harness.press("Ctrl+M").await;
    (root, harness)
}

fn dialog(harness: &Harness) -> &MultiRenameDialog {
    harness.app.multi_rename.as_ref().expect("the dialog")
}

async fn send(harness: &mut Harness, message: MrMsg) {
    harness.send(Message::MultiRename(message)).await;
}

async fn names(harness: &mut Harness, message: NamesMsg) {
    send(harness, MrMsg::Names(message)).await;
}

/// The new names of the table's rows.
fn news(harness: &Harness) -> Vec<String> {
    let list = &dialog(harness).list;
    (0..list.len())
        .filter_map(|index| list.row(index))
        .map(|(_, item)| item.map(|item| item.new.clone()).unwrap_or_default())
        .collect()
}

async fn run_to_end(harness: &mut Harness) {
    let job = dialog(harness)
        .running
        .as_ref()
        .map(|running| running.job)
        .expect("a running job");
    tokio::time::timeout(
        Duration::from_secs(10),
        harness.app.engine().ops().wait(job),
    )
    .await
    .expect("in time")
    .expect("known");
    harness.settle().await;
}

fn read(root: &Path, name: &str) -> Option<String> {
    fs::read_to_string(root.join(name)).ok()
}

#[tokio::test]
async fn edited_names_are_taken_back() {
    let (root, mut harness) = open().await;
    // The editor saved the file (UTF-8 with a BOM, CRLF) the dialog wrote.
    let file = root.path().join("edited-names.txt");
    fs::write(&file, b"\xEF\xBB\xBFfirst.txt\r\nsecond.txt\r\n").expect("write");
    send(
        &mut harness,
        MrMsg::Answer(Answer::Editing(Ok(file.clone()))),
    )
    .await;
    assert_eq!(dialog(&harness).edited.as_deref(), Some(file.as_path()));
    // Nothing else is started while the edited names wait.
    names(&mut harness, NamesMsg::Edit).await;
    assert_eq!(dialog(&harness).edited.as_deref(), Some(file.as_path()));

    names(&mut harness, NamesMsg::Take).await;
    harness.settle().await;
    let opened = dialog(&harness);
    assert_eq!(opened.edited, None);
    assert_eq!(
        opened.form.names,
        Some(vec!["first.txt".to_owned(), "second.txt".to_owned()])
    );
    assert_eq!(news(&harness), ["first.txt", "second.txt"]);
    assert!(!file.exists(), "the editor's file is removed");
}

#[tokio::test]
async fn cancel_drops_the_edited_file() {
    let (root, mut harness) = open().await;
    let file = root.path().join("edited-names.txt");
    fs::write(&file, "x\n").expect("write");
    send(
        &mut harness,
        MrMsg::Answer(Answer::Editing(Ok(file.clone()))),
    )
    .await;
    names(&mut harness, NamesMsg::Cancel).await;
    assert_eq!(dialog(&harness).edited, None);
    assert_eq!(dialog(&harness).form.names, None);
    assert!(!file.exists());
}

#[tokio::test]
async fn names_from_a_file_lock_the_masks_until_cleared() {
    let (root, mut harness) = open().await;
    let file = root.path().join("list.txt");
    fs::write(&file, "one.txt\n").expect("write");
    names(&mut harness, NamesMsg::File(file.display().to_string())).await;
    harness.settle().await;
    assert_eq!(news(&harness), ["one.txt", "c.txt"]);
    let conflicts = dialog(&harness)
        .list
        .row(1)
        .and_then(|(_, item)| item)
        .and_then(|item| item.conflict);
    assert_eq!(conflicts, Some(rc_engine::Conflict::NamesMissing));
    // The placeholders do not reach a locked mask.
    send(
        &mut harness,
        MrMsg::Insert(crate::app::multi_rename::Placeholder::Counter),
    )
    .await;
    assert_eq!(dialog(&harness).form.name_mask, "[N]");

    names(&mut harness, NamesMsg::Clear).await;
    harness.settle().await;
    assert_eq!(dialog(&harness).form.names, None);
    assert_eq!(news(&harness), ["a.txt", "c.txt"]);
}

#[tokio::test]
async fn names_from_the_clipboard_and_x_in_a_mask() {
    let (_root, mut harness) = open().await;
    let clipboard = |usage, text: &str| NamesMsg::Clipboard(usage, Some(text.to_owned()));
    names(
        &mut harness,
        clipboard(ClipboardUse::Names, "p.txt\nq.txt\n"),
    )
    .await;
    harness.settle().await;
    assert_eq!(news(&harness), ["p.txt", "q.txt"]);
    names(&mut harness, NamesMsg::Clear).await;

    send(
        &mut harness,
        MrMsg::Text(Field::NameMask, "[N]_[X+]".to_owned()),
    )
    .await;
    names(
        &mut harness,
        clipboard(ClipboardUse::Mask { debounce: false }, "trip: 1"),
    )
    .await;
    harness.settle().await;
    assert_eq!(news(&harness), ["a_trip_ 1.txt", "c_trip_ 1.txt"]);
}

#[tokio::test]
async fn f5_moves_on_to_the_next_step_and_undo_goes_back_step_by_step() {
    let (root, mut harness) = open().await;
    assert!(!dialog(&harness).can_next_step(), "no run yet");
    send(
        &mut harness,
        MrMsg::Text(Field::NameMask, "x_[N]".to_owned()),
    )
    .await;
    harness.settle().await;
    harness.press("Enter").await;
    run_to_end(&mut harness).await;
    assert_eq!(read(root.path(), "x_a.txt").as_deref(), Some("a.txt"));
    assert!(dialog(&harness).can_next_step());

    harness.press("F5").await;
    harness.settle().await;
    let next = dialog(&harness);
    assert_eq!(next.list.names(), ["x_a.txt", "x_c.txt"]);
    assert_eq!(next.form.name_mask, "[N]", "the rule is back to <Default>");
    assert!(!next.can_next_step());

    send(
        &mut harness,
        MrMsg::Text(Field::NameMask, "[N]_y".to_owned()),
    )
    .await;
    harness.settle().await;
    harness.press("Enter").await;
    run_to_end(&mut harness).await;
    assert_eq!(read(root.path(), "x_c_y.txt").as_deref(), Some("c.txt"));

    harness.press("Alt+U").await;
    run_to_end(&mut harness).await;
    assert_eq!(read(root.path(), "x_c.txt").as_deref(), Some("c.txt"));
    assert_eq!(dialog(&harness).list.names(), ["x_a.txt", "x_c.txt"]);
    harness.press("Alt+U").await;
    run_to_end(&mut harness).await;
    assert_eq!(read(root.path(), "c.txt").as_deref(), Some("c.txt"));
    assert_eq!(dialog(&harness).list.names(), ["a.txt", "c.txt"]);
}

#[tokio::test]
async fn shift_f10_and_alt_m_open_the_more_row() {
    let (_root, mut harness) = open().await;
    assert!(!dialog(&harness).more);
    harness.press("Shift+F10").await;
    assert!(dialog(&harness).more);
    harness.press("Alt+M").await;
    assert!(!dialog(&harness).more);
}
