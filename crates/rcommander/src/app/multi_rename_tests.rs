//! The multi-rename dialog through `apply`, against a real engine with a database: Ctrl+M opens
//! it on the selection, every input becomes a rule the engine previews, conflicts block Start,
//! the order drives the counter, a run renames, selects, logs and is undone, presets keep every
//! field. The harness runs the dialog's calls inline (a preview after its pause).

use std::{fs, time::Duration};

use rc_db::Db;
use rc_engine::{CaseMode, Command, Conflict, InvalidName, PanelId};
use tempfile::TempDir;

use super::{CaseChoice, Field, MrMsg, MultiRenameDialog, PresetChoice, Preview};
use crate::app::harness::headless_engine;
use crate::app::{Message, harness::Harness};

/// `root/` with `a.txt`, `b.txt` and `c.txt`, each holding its own name; an engine on it with a
/// database, the left panel's selection `a.txt` and `c.txt`.
async fn setup() -> (TempDir, Harness) {
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
    (root, harness)
}

fn dialog(harness: &Harness) -> &MultiRenameDialog {
    harness.app.multi_rename.as_ref().expect("the dialog")
}

async fn send(harness: &mut Harness, message: MrMsg) {
    harness.send(Message::MultiRename(message)).await;
}

async fn type_in(harness: &mut Harness, field: Field, text: &str) {
    send(harness, MrMsg::Text(field, text.to_owned())).await;
}

/// `(old, new)` of every row of the table.
fn rows(harness: &Harness) -> Vec<(String, String)> {
    let list = &dialog(harness).list;
    (0..list.len())
        .filter_map(|index| list.row(index))
        .map(|(old, item)| {
            (
                old.to_owned(),
                item.map(|item| item.new.clone()).unwrap_or_default(),
            )
        })
        .collect()
}

fn pair(old: &str, new: &str) -> (String, String) {
    (old.to_owned(), new.to_owned())
}

/// Waits for the dialog's running job, then lets the window hear its end.
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

#[tokio::test]
async fn ctrl_m_opens_on_the_selection_and_escape_closes_without_a_change() {
    let (root, mut harness) = setup().await;
    harness.press("Ctrl+M").await;
    let opened = dialog(&harness);
    assert!(opened.open);
    assert_eq!(opened.list.names(), ["a.txt", "c.txt"], "panel order");
    assert_eq!(
        opened.preview,
        Preview::Ready {
            changes: 0,
            conflicts: 0
        }
    );
    assert!(!opened.can_start(), "nothing to rename");

    let before = harness.app.engine().panel(PanelId::Left).cursor;
    harness.press("ArrowDown").await;
    assert_eq!(harness.app.engine().panel(PanelId::Left).cursor, before);

    harness.press("Escape").await;
    assert!(harness.app.multi_rename.is_none());
    assert_eq!(
        fs::read_to_string(root.path().join("a.txt")).expect("a"),
        "a.txt"
    );
}

#[tokio::test]
async fn every_input_previews_and_a_conflict_blocks_start() {
    let (_root, mut harness) = setup().await;
    harness.press("Ctrl+M").await;
    type_in(&mut harness, Field::NameMask, "same").await;
    assert_eq!(
        rows(&harness),
        [pair("a.txt", "same.txt"), pair("c.txt", "same.txt")]
    );
    assert!(matches!(
        dialog(&harness).preview,
        Preview::Ready { conflicts: 2, .. }
    ));
    let conflict = dialog(&harness)
        .list
        .row(0)
        .and_then(|(_, item)| item)
        .and_then(|item| item.conflict);
    assert_eq!(conflict, Some(Conflict::Duplicate { other: 1 }));
    harness.press("Enter").await;
    assert!(!dialog(&harness).busy(), "Start is blocked");

    type_in(&mut harness, Field::NameMask, "pic[C]").await;
    send(&mut harness, MrMsg::Case(CaseChoice(CaseMode::Upper))).await;
    assert_eq!(
        rows(&harness),
        [pair("a.txt", "PIC1.TXT"), pair("c.txt", "PIC2.TXT")]
    );
    assert!(dialog(&harness).can_start());

    type_in(&mut harness, Field::NameMask, "pic[X]").await;
    let Preview::Failed(error) = &dialog(&harness).preview else {
        panic!("a mask error");
    };
    assert_eq!(error.mask, Some((rc_engine::MaskField::Name, 4)));
    assert!(!dialog(&harness).can_start());
}

#[tokio::test]
async fn a_path_out_of_the_folder_is_a_conflict_with_its_reason() {
    let (_root, mut harness) = setup().await;
    harness.press("Ctrl+M").await;
    type_in(&mut harness, Field::NameMask, r"..\[N]").await;
    let conflict = dialog(&harness)
        .list
        .row(0)
        .and_then(|(_, item)| item)
        .and_then(|item| item.conflict);
    assert_eq!(
        conflict,
        Some(Conflict::InvalidName {
            reason: InvalidName::ParentDir
        })
    );
    assert!(!dialog(&harness).can_start());
}

#[tokio::test]
async fn moving_a_row_changes_the_counter() {
    let (_root, mut harness) = setup().await;
    harness.press("Ctrl+M").await;
    type_in(&mut harness, Field::NameMask, "[C]_[N]").await;
    assert_eq!(
        rows(&harness),
        [pair("a.txt", "1_a.txt"), pair("c.txt", "2_c.txt")]
    );
    harness.press("Ctrl+ArrowDown").await;
    assert_eq!(
        rows(&harness),
        [pair("c.txt", "1_c.txt"), pair("a.txt", "2_a.txt")]
    );
}

#[tokio::test]
async fn a_run_renames_selects_logs_and_undo_brings_the_names_back() {
    let (root, mut harness) = setup().await;
    let log = root.path().join("logs").join("rename.log");
    fs::create_dir(root.path().join("logs")).expect("mkdir");
    harness.press("Ctrl+M").await;
    type_in(&mut harness, Field::NameMask, "pic[C]").await;
    send(&mut harness, MrMsg::Log(true)).await;
    type_in(&mut harness, Field::LogPath, &log.display().to_string()).await;
    harness.press("Enter").await;
    run_to_end(&mut harness).await;

    for (name, content) in [("pic1.txt", "a.txt"), ("pic2.txt", "c.txt")] {
        assert_eq!(
            fs::read_to_string(root.path().join(name)).expect(name),
            content
        );
    }
    assert_eq!(dialog(&harness).list.names(), ["pic1.txt", "pic2.txt"]);
    let selection = harness.app.engine().panel(PanelId::Left).selection;
    let selected: Vec<&str> = selection.iter().map(String::as_str).collect();
    assert_eq!(selected, ["pic1.txt", "pic2.txt"]);
    let written = fs::read_to_string(&log).expect("the log");
    assert_eq!(written.lines().count(), 2, "{written}");
    assert!(
        written.lines().all(|line| line.contains(" -> ")),
        "{written}"
    );

    harness.press("Alt+U").await;
    run_to_end(&mut harness).await;
    assert_eq!(
        fs::read_to_string(root.path().join("a.txt")).expect("a"),
        "a.txt"
    );
    assert!(!root.path().join("pic1.txt").exists());
    assert_eq!(dialog(&harness).list.names(), ["a.txt", "c.txt"]);
}

#[tokio::test]
async fn a_saved_preset_restores_every_field() {
    let (_root, mut harness) = setup().await;
    harness.press("Ctrl+M").await;
    type_in(&mut harness, Field::NameMask, "[Y]-[M]-[D]_[C]").await;
    type_in(&mut harness, Field::ExtensionMask, "[L][E]").await;
    type_in(&mut harness, Field::Find, "a|c").await;
    type_in(&mut harness, Field::Replace, "x|y").await;
    harness.press("Alt+1").await;
    type_in(&mut harness, Field::CounterWidth, "3").await;
    send(&mut harness, MrMsg::Case(CaseChoice(CaseMode::Lower))).await;
    let saved = dialog(&harness).form.clone();
    assert!(saved.first_only);

    harness.press("F2").await;
    assert!(
        dialog(&harness).status != super::Status::None,
        "a preset needs a name"
    );
    type_in(&mut harness, Field::PresetName, "Photos").await;
    harness.press("F2").await;
    assert_eq!(dialog(&harness).presets.len(), 1);

    harness.press("Alt+D").await;
    assert_eq!(dialog(&harness).form.name_mask, "[N]");
    harness.press("Escape").await;
    harness.press("Ctrl+M").await;
    assert_eq!(dialog(&harness).presets.len(), 1, "read again at the open");
    send(
        &mut harness,
        MrMsg::Preset(PresetChoice::Named("Photos".into())),
    )
    .await;
    assert_eq!(dialog(&harness).form.rule(), saved.rule());
    assert_eq!(dialog(&harness).form.case, CaseChoice(CaseMode::Lower));
}

/// RC-080-38: Ctrl+M on `..` without a selection opens nothing - not for the first entry the
/// engine's cursor is on, as in Total Commander.
#[tokio::test]
async fn ctrl_m_on_the_parent_row_without_a_selection_opens_nothing() {
    let (_root, mut harness) = setup().await;
    harness
        .app
        .engine()
        .dispatch(Command::ToggleSelection {
            panel: Some(PanelId::Left),
            names: vec!["a.txt".into(), "c.txt".into()],
        })
        .await
        .expect("deselects");
    harness.settle().await;
    harness.send(Message::Click(PanelId::Left, 0)).await;
    let pane = harness.app.pane(PanelId::Left);
    assert!(pane.on_parent() && pane.state.selection.is_empty());
    harness.press("Ctrl+M").await;
    assert!(harness.app.multi_rename.is_none());
}
