use rc_engine::{Change, RenameItem};

use super::{ROWS, RenameList};

fn names(list: &RenameList) -> Vec<&str> {
    list.names().iter().map(String::as_str).collect()
}

fn item(old: &str, new: &str) -> RenameItem {
    RenameItem {
        old: old.to_owned(),
        new: new.to_owned(),
        change: if old == new {
            Change::Unchanged
        } else {
            Change::Rename
        },
        conflict: None,
    }
}

fn list(of: &[&str]) -> RenameList {
    RenameList::new(of.iter().map(|name| (*name).to_owned()).collect())
}

#[test]
fn a_preview_of_another_order_is_not_shown() {
    let mut list = list(&["a", "b"]);
    list.set_items(vec![item("b", "x"), item("a", "y")]);
    assert_eq!(list.row(0), Some(("a", None)));
    list.set_items(vec![item("a", "y"), item("b", "b")]);
    assert_eq!(
        list.row(0)
            .and_then(|(_, item)| item)
            .map(|item| item.new.as_str()),
        Some("y")
    );
    assert_eq!(list.changes(), [("a".to_owned(), "y".to_owned())]);
}

#[test]
fn moving_a_row_changes_the_order_and_drops_the_preview() {
    let mut list = list(&["a", "b", "c"]);
    list.set_items(vec![item("a", "1"), item("b", "2"), item("c", "3")]);
    assert!(!list.move_row(-1), "the first row cannot go up");
    assert!(list.move_row(1));
    assert_eq!(names(&list), ["b", "a", "c"]);
    assert_eq!(list.cursor(), 1);
    assert_eq!(list.row(1), Some(("a", None)));
    list.point_at(2);
    assert!(!list.move_row(1), "the last row cannot go down");
}

#[test]
fn sorting_by_name_turns_around_and_keeps_the_cursor_on_its_entry() {
    let mut list = list(&["b.txt", "C.txt", "a.txt"]);
    list.point_at(1);
    list.sort_by_name();
    assert_eq!(names(&list), ["a.txt", "b.txt", "C.txt"]);
    assert_eq!(list.cursor(), 2);
    list.sort_by_name();
    assert_eq!(names(&list), ["C.txt", "b.txt", "a.txt"]);
    assert_eq!(list.cursor(), 0);
}

#[test]
fn only_the_rows_on_screen_are_visible() {
    let many: Vec<String> = (0..10_000).map(|n| format!("f{n}")).collect();
    let mut list = RenameList::new(many);
    assert_eq!(list.visible(), 0..ROWS);
    list.step(500);
    assert_eq!(list.visible(), (501 - ROWS)..501);
    list.scroll(1_000_000);
    assert_eq!(list.visible().end, 10_000);
    list.set_names(vec!["only".to_owned()]);
    assert_eq!(list.cursor(), 0);
    assert_eq!(list.visible(), 0..1);
}

/// The table takes the dialog's height: as many rows show as fit.
#[test]
fn the_table_shows_as_many_rows_as_its_height_holds() {
    let many: Vec<String> = (0..100).map(|n| format!("f{n}")).collect();
    let mut list = RenameList::new(many);
    assert_eq!(list.page(), ROWS);
    list.set_page(30);
    assert_eq!(list.visible(), 0..30);
    list.step(40);
    assert_eq!(list.visible(), 11..41);
    list.scroll(1_000);
    assert_eq!(list.visible(), 70..100);
    list.set_page(5);
    assert_eq!(list.visible(), 40..45, "the cursor stays on screen");
}
