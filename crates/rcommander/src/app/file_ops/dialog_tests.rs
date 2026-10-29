use std::{fs, path::MAIN_SEPARATOR};

use rc_engine::PanelState;

use super::*;
use crate::app::harness::headless_engine;

/// A panel on a directory with `docs/`, `a.txt` and `b.tar.gz`; the cursor on `name`.
async fn state(cursor: &str, selected: &[&str]) -> (tempfile::TempDir, PanelState) {
    let root = tempfile::tempdir().expect("temp dir");
    fs::create_dir(root.path().join("docs")).expect("mkdir");
    fs::write(root.path().join("a.txt"), b"a").expect("write");
    fs::write(root.path().join("b.tar.gz"), b"b").expect("write");
    let engine = headless_engine(root.path()).await;
    let mut state = engine.panel(PanelId::Left);
    state.cursor = state.index_of(cursor).expect("an entry");
    state.selection = selected.iter().map(|name| (*name).to_owned()).collect();
    (root, state)
}

fn other() -> &'static str {
    "/elsewhere"
}

#[tokio::test]
async fn f5_takes_the_cursor_entry_and_offers_the_other_panel() {
    let (_root, state) = state("a.txt", &[]).await;
    let dialog =
        Dialog::open(FileDialog::Copy, PanelId::Left, &state, false, other()).expect("a dialog");
    assert_eq!(dialog.count, 1);
    assert_eq!(dialog.single.as_deref(), Some("a.txt"));
    assert_eq!(dialog.text, format!("{}{MAIN_SEPARATOR}", other()));
    let (command, follow) = dialog.submit().expect("a command");
    assert_eq!(
        command,
        Command::OpsCopyTo {
            panel: Some(PanelId::Left),
            sources: vec!["a.txt".into()],
            target: dialog.text.clone(),
            options: JobOptions::default(),
            queue: None,
        }
    );
    assert_eq!(follow, FollowUp::Transfer { deselect: true });
}

/// F2 puts the copy into the queue list chosen last, while that list is still offered
/// (RC-010-15).
#[tokio::test]
async fn f2_queues_into_the_chosen_list_while_it_is_offered() {
    let (_root, state) = state("a.txt", &[]).await;
    let mut dialog =
        Dialog::open(FileDialog::Copy, PanelId::Left, &state, false, other()).expect("a dialog");
    let list = |id: u64| rc_engine::QueueList {
        id,
        name: format!("list {id}"),
        started: false,
        jobs: 0,
    };
    dialog.offer_lists(vec![list(0), list(4)], 4);
    assert_eq!(dialog.queue, 4);
    let (command, _) = dialog.submit_queued().expect("a command");
    assert!(
        matches!(command, Command::OpsCopyTo { queue: Some(4), .. }),
        "{command:?}"
    );
    let (command, _) = dialog.submit().expect("a command");
    assert!(
        matches!(command, Command::OpsCopyTo { queue: None, .. }),
        "OK uses the default list"
    );
    dialog.offer_lists(vec![list(0)], 4);
    assert_eq!(
        dialog.queue,
        rc_engine::DEFAULT_LIST,
        "a list gone falls back"
    );
}

/// The selection is fixed when the dialog opens (RC-080-44): OK hands its names over, not
/// whatever is selected by then.
#[tokio::test]
async fn a_selection_is_fixed_when_the_dialog_opens_and_counted() {
    let (_root, state) = state("a.txt", &["a.txt", "docs"]).await;
    let mut dialog =
        Dialog::open(FileDialog::Move, PanelId::Left, &state, false, other()).expect("a dialog");
    assert_eq!((dialog.count, dialog.single.as_deref()), (2, None));
    dialog.preset = Preset::OverwriteOlder;
    dialog.verify = true;
    assert!(dialog.keep_attributes, "on when the dialog opens");
    dialog.keep_attributes = false;
    let (command, _) = dialog.submit().expect("a command");
    assert_eq!(
        command,
        Command::OpsMoveTo {
            panel: Some(PanelId::Left),
            sources: vec!["a.txt".into(), "docs".into()],
            target: dialog.text.clone(),
            options: JobOptions {
                on_conflict: Some(ConflictChoice::OverwriteOlder),
                verify: true,
                keep_attributes: false,
                ..JobOptions::default()
            },
            queue: None,
        }
    );
}

/// The audit's D8: the selection changed while F5's dialog was open - here by an MCP call - and
/// OK copied the new one. OK copies what the dialog showed.
#[tokio::test]
async fn ok_copies_what_the_dialog_showed_not_the_selection_since() {
    let (root, _state) = state("a.txt", &[]).await;
    let mut harness = crate::app::harness::Harness::headless(root.path()).await;
    let select = |name: &str| Command::ToggleSelection {
        panel: Some(PanelId::Left),
        names: vec![name.into()],
    };
    let engine = harness.app.engine().clone();
    engine.dispatch(select("a.txt")).await.expect("selects");
    harness.settle().await;
    harness.press("F5").await;
    engine.dispatch(select("b.tar.gz")).await.expect("selects");
    harness.settle().await;
    assert_eq!(harness.app.pane(PanelId::Left).state.selection.len(), 2);

    let start = super::super::FileOpsMsg::Start { background: false };
    let effects = harness.app.apply(crate::app::Message::FileOps(start));
    let [crate::app::Effect::Ops(submit)] = effects.as_slice() else {
        panic!("one job, got {} effects", effects.len());
    };
    assert!(
        matches!(&submit.command, Command::OpsCopyTo { sources, .. } if sources == &["a.txt"]),
        "{:?}",
        submit.command
    );
}

/// A server in the other panel is offered as its location, so the target stays on it
/// (RC-060-05).
#[tokio::test]
async fn f5_offers_a_server_as_its_location() {
    let (_root, state) = state("a.txt", &[]).await;
    for (other, offered) in [
        ("sftp://box/srv/www", "sftp://box/srv/www/"),
        ("sftp://box/", "sftp://box/"),
    ] {
        let dialog =
            Dialog::open(FileDialog::Copy, PanelId::Left, &state, false, other).expect("a dialog");
        assert_eq!(dialog.text, offered);
    }
}

#[tokio::test]
async fn nothing_to_work_on_opens_no_dialog() {
    let (_root, state) = state("a.txt", &[]).await;
    for kind in [
        FileDialog::Copy,
        FileDialog::Move,
        FileDialog::Delete,
        FileDialog::CopyHere,
        FileDialog::Rename,
    ] {
        assert_eq!(
            Dialog::open(kind, PanelId::Left, &state, true, other()),
            None,
            "{kind:?} on the .. row"
        );
    }
    assert!(Dialog::open(FileDialog::Mkdir, PanelId::Left, &state, true, other()).is_some());
    assert_eq!(InlineRename::open(PanelId::Left, &state, true), None);
}

#[tokio::test]
async fn shift_f5_offers_the_name_with_its_stem_selected() {
    let (_root, state) = state("b.tar.gz", &[]).await;
    let dialog = Dialog::open(FileDialog::CopyHere, PanelId::Left, &state, false, other())
        .expect("a dialog");
    assert_eq!(dialog.text, "b.tar.gz");
    assert_eq!(dialog.selection(), Some(5));
    let (command, _) = dialog.submit().expect("a command");
    assert!(matches!(
        command,
        Command::OpsCopyTo { ref sources, ref target, .. } if sources == &["b.tar.gz"] && target == "b.tar.gz"
    ));
}

#[tokio::test]
async fn delete_and_delete_for_good_pick_the_mode() {
    let (_root, state) = state("docs", &[]).await;
    for (kind, mode) in [
        (FileDialog::Delete, DeleteMode::Trash),
        (FileDialog::DeletePermanent, DeleteMode::Permanent),
    ] {
        let dialog = Dialog::open(kind, PanelId::Left, &state, false, other()).expect("a dialog");
        assert!(!dialog.has_field());
        let (command, follow) = dialog.submit().expect("a command");
        assert_eq!(
            command,
            Command::OpsDelete(rc_engine::DeleteRequest {
                panel: Some(PanelId::Left),
                sources: vec!["docs".into()],
                mode,
                on_error: ErrorPolicy::Ask,
                queue: None,
            })
        );
        assert_eq!(follow, FollowUp::Refresh);
    }
}

#[tokio::test]
async fn f7_makes_nested_folders_and_puts_the_cursor_on_the_first() {
    let (_root, state) = state("a.txt", &[]).await;
    let mut dialog =
        Dialog::open(FileDialog::Mkdir, PanelId::Left, &state, false, other()).expect("a dialog");
    dialog.text.clear();
    assert_eq!(dialog.submit(), Err("invalid_name"), "an empty name");
    dialog.text = "a/b/c".into();
    let (command, follow) = dialog.submit().expect("a command");
    assert_eq!(
        command,
        Command::OpsMkdir {
            panel: Some(PanelId::Left),
            path: "a/b/c".into(),
        }
    );
    assert_eq!(follow, FollowUp::Cursor(Some("a".into())));
}

/// F7 offers the name under the cursor, a file's or a folder's, all of it selected; on `..`
/// nothing.
#[tokio::test]
async fn f7_offers_the_name_under_the_cursor() {
    for name in ["a.txt", "docs"] {
        let (_root, state) = state(name, &[]).await;
        let dialog = Dialog::open(FileDialog::Mkdir, PanelId::Left, &state, false, other())
            .expect("a dialog");
        assert_eq!((dialog.text.as_str(), dialog.selection()), (name, None));
    }
    let (_root, state) = state("a.txt", &[]).await;
    let dialog =
        Dialog::open(FileDialog::Mkdir, PanelId::Left, &state, true, other()).expect("a dialog");
    assert_eq!(dialog.text, "", "on the .. row");
}

#[tokio::test]
async fn shift_f4_offers_the_cursor_file_and_opens_the_editor() {
    let (_root, state) = state("a.txt", &[]).await;
    let dialog =
        Dialog::open(FileDialog::NewText, PanelId::Left, &state, false, other()).expect("a dialog");
    assert_eq!(dialog.text, "a.txt");
    let (command, _) = dialog.submit().expect("a command");
    assert_eq!(
        command,
        Command::OpenExternal {
            panel: Some(PanelId::Left),
            path: Some("a.txt".into()),
            program: ExternalProgram::Editor,
            create: true,
        }
    );
}

#[test]
fn the_first_part_of_a_path_is_where_the_cursor_goes() {
    assert_eq!(first_part("new"), Some("new".into()));
    assert_eq!(first_part("a/b/c"), Some("a".into()));
    assert_eq!(first_part("../x"), None);
    assert_eq!(first_part(&format!("{MAIN_SEPARATOR}abs")), None);
}

#[tokio::test]
async fn shift_f6_renames_with_a_checked_name() {
    let (_root, state) = state("b.tar.gz", &[]).await;
    let mut rename = InlineRename::open(PanelId::Left, &state, false).expect("an editor");
    assert_eq!(
        rename.selected,
        "b.tar".chars().count(),
        "the extension is not selected"
    );
    assert_eq!(
        rename.submit(),
        Ok(None),
        "an unchanged name renames nothing"
    );
    rename.text = "bad/name".into();
    assert_eq!(rename.submit(), Err("invalid_name"));
    rename.text = " c.tar.gz ".into();
    let (command, follow) = rename.submit().expect("valid").expect("a rename");
    assert_eq!(
        command,
        Command::OpsMoveTo {
            panel: Some(PanelId::Left),
            sources: vec!["b.tar.gz".into()],
            target: "c.tar.gz".into(),
            options: JobOptions::default(),
            queue: None,
        }
    );
    assert_eq!(follow, FollowUp::Cursor(Some("c.tar.gz".into())));
}
