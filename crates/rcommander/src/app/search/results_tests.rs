use rc_engine::{Entry, EntryKind, VfsPath};

use super::*;

fn hit(n: usize) -> SearchHit {
    let name = format!("f{n}.txt");
    SearchHit {
        path: VfsPath::local(format!("/data/{name}")),
        entry: Entry {
            name,
            kind: EntryKind::File,
            size: 1,
            modified: None,
            created: None,
            attributes: rc_vfs::Attributes::default(),
            hidden: false,
            owner: None,
        },
    }
}

fn results(count: usize) -> Results {
    let mut results = Results::default();
    for n in 0..count {
        assert!(results.push(hit(n)));
    }
    results
}

#[test]
fn the_cursor_stays_on_screen_and_inside_the_hits() {
    let mut list = results(100);
    assert_eq!(list.visible(), 0..ROWS);
    list.step(ROWS as isize);
    assert_eq!(list.cursor(), ROWS);
    assert_eq!(list.visible(), 1..ROWS + 1);
    list.step(1000);
    assert_eq!(list.cursor(), 99);
    assert_eq!(list.visible(), 100 - ROWS..100);
    list.step(-1000);
    assert_eq!(list.cursor(), 0);
    assert_eq!(list.visible(), 0..ROWS);
}

#[test]
fn the_wheel_moves_the_view_not_the_cursor() {
    let mut list = results(30);
    list.scroll(5);
    assert_eq!(list.visible(), 5..5 + ROWS);
    assert_eq!(list.cursor(), 0);
    list.scroll(100);
    assert_eq!(list.visible(), 30 - ROWS..30);
    list.point_at(29);
    assert_eq!(
        list.current().map(|hit| hit.entry.name.as_str()),
        Some("f29.txt")
    );
    list.point_at(30);
    assert_eq!(list.cursor(), 29, "no hit there");
}

#[test]
fn a_short_list_shows_what_it_has() {
    let mut list = results(3);
    assert_eq!(list.visible(), 0..3);
    list.scroll(4);
    assert_eq!(list.visible(), 0..3);
    assert!(Results::default().current().is_none());
}

/// The list takes the dialog's height: as many hits show as fit, and a new search keeps it.
#[test]
fn the_list_shows_as_many_hits_as_its_height_holds() {
    let mut list = results(100);
    list.set_page(30);
    assert_eq!(list.visible(), 0..30);
    list.step(40);
    assert_eq!(list.visible(), 11..41);
    list.set_page(60);
    assert_eq!(list.visible(), 11..71);
    list.scroll(1_000);
    assert_eq!(list.visible(), 40..100);
    list.set_page(5);
    assert_eq!(
        list.visible(),
        40..45,
        "the view keeps its top, the cursor on it"
    );
    list.clear();
    assert!(list.is_empty());
    assert_eq!(list.page(), 5);
    list.set_page(0);
    assert_eq!(list.page(), 1);
}
