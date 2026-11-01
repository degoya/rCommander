use super::*;

fn rows() -> Vec<WorkspaceRow> {
    [(5, "Servers"), (2, "Local and server"), (9, "Web servers")]
        .into_iter()
        .map(|(id, name)| WorkspaceRow {
            id,
            name: name.to_owned(),
            active: id == 9,
        })
        .collect()
}

/// The filter keeps the order and finds every word anywhere in a name, ignoring case.
#[test]
fn the_filter_narrows_by_every_word() {
    assert_eq!(shown(&rows(), ""), [5, 2, 9]);
    assert_eq!(shown(&rows(), "SERVER"), [5, 2, 9]);
    assert_eq!(shown(&rows(), "web serv"), [9]);
    assert_eq!(shown(&rows(), "local  server"), [2]);
    assert!(shown(&rows(), "nothing").is_empty());
}

/// A new list keeps the cursor on its workspace, else on the active one, else on the first.
#[test]
fn the_cursor_stays_on_its_workspace() {
    let all = [5, 2, 9];
    assert_eq!(keep(&all, Some(2), Some(9)), Some(2));
    assert_eq!(
        keep(&all, Some(4), Some(9)),
        Some(9),
        "deleted: the active one"
    );
    assert_eq!(keep(&[5, 2], Some(4), Some(9)), Some(5), "else the first");
    assert_eq!(keep(&all, None, None), Some(5));
    assert_eq!(keep(&[], Some(2), Some(9)), None);
}

/// The arrows and the page keys move the cursor, never past either end.
#[test]
fn the_cursor_steps_within_the_list() {
    let all = [5, 2, 9];
    assert_eq!(step(&all, Some(5), 1), Some(2));
    assert_eq!(step(&all, Some(2), -1), Some(5));
    assert_eq!(step(&all, Some(5), -1), Some(5));
    assert_eq!(step(&all, Some(2), PAGE), Some(9));
    assert_eq!(step(&all, Some(9), -PAGE), Some(5));
    assert_eq!(step(&all, None, 1), Some(5), "from no row: the first");
    assert_eq!(step(&[], Some(5), 1), None);
}

/// Ctrl+Up and Ctrl+Down move a workspace one place among all, not past either end.
#[test]
fn a_move_goes_one_place() {
    let rows = rows();
    assert_eq!(moved_to(&rows, 2, true), Some(0));
    assert_eq!(moved_to(&rows, 2, false), Some(2));
    assert_eq!(moved_to(&rows, 5, true), None);
    assert_eq!(moved_to(&rows, 9, false), None);
    assert_eq!(moved_to(&rows, 4, true), None, "not listed");
}

/// A new workspace gets the first number no name has, ignoring case.
#[test]
fn a_new_name_is_free() {
    let mut rows = rows();
    assert_eq!(new_name(&rows, "Workspace"), "Workspace 1");
    rows[0].name = "workspace 1".to_owned();
    assert_eq!(new_name(&rows, "Workspace"), "Workspace 2");
    assert_eq!(new_name(&[], "Workspace"), "Workspace 1");
}
