//! The thumbnails' grid in the panel (RC-100-89): lines of tiles, `..` the first tile.

use rc_engine::{
    Command, CursorStep, CursorTarget, Entry, EntryKind, ListMode, PanelState, PanelView,
    SortOrder, VfsPath,
};
use rc_vfs::Attributes;

use super::Pane;

fn state(count: usize, cursor: usize) -> PanelState {
    let entries: Vec<Entry> = (0..count)
        .map(|i| Entry {
            name: format!("photo_{i:04}.jpg"),
            kind: EntryKind::File,
            size: 1,
            modified: None,
            created: None,
            accessed: None,
            attributes: Attributes::default(),
            hidden: false,
            owner: None,
        })
        .collect();
    PanelState {
        path: VfsPath::local("/photos"),
        entries: entries.into(),
        cursor,
        selection: Default::default(),
        view: PanelView::Files,
        sort: SortOrder::default(),
        mode: ListMode::Thumbnails,
        show_hidden: false,
        remembered: Default::default(),
        filter: Default::default(),
        flat: None,
        archive: None,
        dir_sizes: Default::default(),
        columns: Default::default(),
        listing: None,
        git: None,
    }
}

/// 100 photos and `..` in lines of four tiles, three lines on screen.
fn grid() -> Pane {
    let mut pane = Pane::new(state(100, 0));
    pane.set_page_rows(3);
    pane.set_tiles(4);
    pane
}

fn step(pane: &mut Pane, step: CursorStep) -> Option<Command> {
    pane.through_parent_row(Command::MoveCursor { panel: None, step })
}

fn to_index(index: usize) -> Option<Command> {
    Some(Command::SetCursor {
        panel: None,
        target: CursorTarget::Index(index),
    })
}

#[test]
fn a_screen_of_lines_and_one_more_is_built() {
    let pane = grid();
    assert!(pane.thumbnails());
    assert_eq!(pane.rows(), 101);
    assert_eq!(pane.columns(), 4, "the scroll bar's page is lines of tiles");
    assert_eq!(
        pane.visible_range(),
        0..16,
        "three lines and the partly shown fourth"
    );
}

#[test]
fn the_steps_go_by_tiles_and_lines_with_the_parent_tile_first() {
    let mut pane = grid();
    assert!(pane.on_parent());
    assert_eq!(
        step(&mut pane, CursorStep::Down),
        to_index(3),
        "row 4 is entry 3"
    );
    pane.show_cursor(state(100, 3));
    assert_eq!(step(&mut pane, CursorStep::Right), to_index(4));
    pane.show_cursor(state(100, 1));
    assert_eq!(
        step(&mut pane, CursorStep::Up),
        None,
        "no line above the first"
    );

    pane.show_cursor(state(100, 5));
    assert_eq!(pane.cursor_row(), 6);
    assert_eq!(step(&mut pane, CursorStep::Up), to_index(1));
    assert_eq!(step(&mut pane, CursorStep::Left), to_index(4));
    assert_eq!(
        step(&mut pane, CursorStep::PageDown),
        to_index(13),
        "two lines"
    );
    assert_eq!(step(&mut pane, CursorStep::Last), to_index(99));

    pane.show_cursor(state(100, 0));
    assert_eq!(
        step(&mut pane, CursorStep::Left),
        Some(Command::MoveCursor {
            panel: None,
            step: CursorStep::First
        }),
        "left of the first tile is `..`"
    );
    assert!(pane.on_parent());
}

#[test]
fn the_view_follows_the_cursor_by_whole_lines() {
    let mut pane = grid();
    pane.show_cursor(state(100, 50));
    // Row 51 is on line 12; three lines show, so the view starts at line 10.
    assert_eq!(pane.top(), 40);
    pane.scroll(-100);
    assert_eq!(pane.top(), 0);
    pane.scroll(2);
    assert_eq!(pane.top(), 8, "two lines of four tiles");
    pane.scroll(1000);
    assert_eq!(pane.max_top(), (26 - 3) * 4);
    assert_eq!(pane.top(), pane.max_top());
    pane.scroll_to(37);
    assert_eq!(pane.top(), 36, "the start of the line row 37 is on");
}

#[test]
fn an_empty_folder_has_nothing_to_step_to() {
    let mut pane = Pane::new(state(0, 0));
    pane.set_tiles(4);
    assert_eq!(step(&mut pane, CursorStep::Down), None);
}
