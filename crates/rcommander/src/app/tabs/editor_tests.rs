use rc_engine::VfsPath;

use super::*;

fn entry(id: i64, parent_id: Option<i64>, folder: bool) -> HotlistEntry {
    HotlistEntry {
        id,
        parent_id,
        name: format!("n{id}"),
        folder,
        path: (!folder).then(|| VfsPath::local(format!("/p{id}"))),
        target: None,
    }
}

/// Folder 1 holds entry 2 and folder 3, which holds entry 4; entry 5 is at the top.
fn editor() -> HotlistEditor {
    HotlistEditor::new(vec![
        entry(1, None, true),
        entry(2, Some(1), false),
        entry(3, Some(1), true),
        entry(4, Some(3), false),
        entry(5, None, false),
    ])
}

#[test]
fn depths_follow_the_folders() {
    assert_eq!(editor().depths, [0, 1, 1, 2, 0]);
}

#[test]
fn choosing_a_row_fills_the_fields() {
    let mut editor = editor();
    editor.choose(1);
    assert_eq!(editor.chosen, Some(2));
    assert_eq!(editor.name, "n2");
    assert_eq!(
        editor.path,
        VfsPath::local("/p2").path.display().to_string()
    );
    assert_eq!(editor.target, "");
}

#[test]
fn saving_an_entry_sends_name_path_and_target() {
    let mut editor = editor();
    editor.choose(4);
    editor.name = "Home".into();
    editor.target = "docs".into();
    let Some(Command::UpdateHotlist(request)) = editor.save() else {
        panic!("an update");
    };
    assert_eq!(request.id, 5);
    assert_eq!(request.name.as_deref(), Some("Home"));
    assert_eq!(request.target.as_deref(), Some("docs"));
    assert!(!request.clear_target);
}

#[test]
fn a_folder_saves_only_its_name() {
    let mut editor = editor();
    editor.choose(0);
    editor.path = "/somewhere".into();
    let Some(Command::UpdateHotlist(request)) = editor.save() else {
        panic!("an update");
    };
    assert_eq!((request.path, request.target), (None, None));
    assert!(!request.clear_target);
}

#[test]
fn new_rows_go_into_the_chosen_folder_or_beside_the_chosen_entry() {
    let mut editor = editor();
    let parent = |command: Command| match command {
        Command::AddHotlist(request) => request.parent_id,
        Command::AddHotlistFolder(request) => request.parent_id,
        other => panic!("an add, got {other:?}"),
    };
    assert_eq!(parent(editor.add_current(PanelId::Left)), None);
    editor.choose(2);
    assert_eq!(parent(editor.add_folder()), Some(3));
    editor.choose(3);
    assert_eq!(parent(editor.add_current(PanelId::Left)), Some(3));
}

#[test]
fn up_and_down_stay_among_the_siblings() {
    let mut editor = editor();
    editor.choose(1);
    assert_eq!(editor.step(-1), None, "already first in folder 1");
    let Some(Command::MoveHotlist(request)) = editor.step(1) else {
        panic!("a move");
    };
    assert_eq!((request.parent_id, request.position), (Some(1), Some(1)));
}

#[test]
fn a_folder_cannot_be_moved_into_itself_or_below() {
    let mut editor = editor();
    editor.choose(0);
    let ids: Vec<Option<i64>> = editor.folders().iter().map(|choice| choice.id).collect();
    assert_eq!(
        ids,
        [None],
        "folders 1 and 3 are folder 1 itself and inside it"
    );
    editor.choose(4);
    let ids: Vec<Option<i64>> = editor.folders().iter().map(|choice| choice.id).collect();
    assert_eq!(ids, [None, Some(1), Some(3)]);
    assert_eq!(editor.move_to(None), None, "already at the top");
    assert!(editor.move_to(Some(3)).is_some());
}

#[test]
fn a_row_gone_after_a_reload_is_no_longer_chosen() {
    let mut editor = editor();
    editor.choose(4);
    editor.reload(vec![entry(1, None, true)]);
    assert_eq!(editor.chosen, None);
    assert_eq!(editor.delete(), None);
}
