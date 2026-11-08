use std::sync::Arc;

use rc_engine::{
    VfsPath,
    compare::{
        CompareSide, Direction, EditSession, Origin, TextDiff, TextOptions, edit::SideFile,
        text::decode,
    },
};

use super::{Ask, Note, Then};
use crate::app::compare::{DiffWindow, Loaded};

fn side(text: &str, name: &str) -> SideFile {
    SideFile {
        file: decode(text.as_bytes()).expect("text"),
        origin: Origin {
            path: VfsPath::local(name),
            size: text.len() as u64,
            modified: None,
        },
        read_only: None,
    }
}

/// The window on two texts, loaded as its task loads them.
fn window(left: &str, right: &str) -> DiffWindow {
    let session = EditSession::new(side(left, "/l.txt"), side(right, "/r.txt"));
    let (l, r) = session.files();
    let diff = TextDiff::compute(l, r, &TextOptions::default());
    let mut window = DiffWindow::new(VfsPath::local("/l.txt"), VfsPath::local("/r.txt"), 1);
    window.set_page(10);
    window.loaded(Ok(Loaded::Text(Arc::new(diff), Box::new(session))));
    window
}

/// The diff of the text as it is now handed over, as the window's task does after a change.
fn recompute(window: &mut DiffWindow) {
    let session = window.session.as_ref().expect("text");
    let (l, r) = session.files();
    let generation = session.generation();
    let diff = TextDiff::compute(l, r, &window.options);
    assert!(window.computed(generation, Arc::new(diff)));
}

fn text(window: &DiffWindow, side: CompareSide) -> String {
    let session = window.session.as_ref().expect("text");
    String::from_utf8(session.encoded(side).expect("encodes")).expect("UTF-8")
}

#[test]
fn the_edit_mode_starts_on_the_first_difference_and_edits_the_line_there() {
    let mut window = window("a\nb\nc\n", "a\nB\nc\n");
    assert!(window.toggle_edit());
    let cursor = window.edit.expect("edit mode");
    assert_eq!((cursor.row, cursor.side), (1, CompareSide::Left));
    assert!(window.start_field(false));
    assert_eq!(
        window.field.as_ref().map(|field| field.text.as_str()),
        Some("b")
    );
    window.type_field("B".into());
    assert!(window.commit_field());
    assert_eq!(text(&window, CompareSide::Left), "a\nB\nc\n");
    // Until the diff of the new text is in, nothing else changes.
    assert!(window.fresh().is_none());
    assert!(!window.copy(Direction::LeftToRight));
    recompute(&mut window);
    assert_eq!(window.changes, 0);
    assert!(window.dirty());
    assert!(window.toggle_edit(), "off again");
    assert!(window.edit.is_none());
}

#[test]
fn a_block_is_copied_and_undone_along_the_cursor() {
    let mut window = window("a\nb\nc\nd\n", "a\nX\nY\nd\n");
    window.toggle_edit();
    window.move_cursor(1);
    assert_eq!(window.edit.map(|cursor| cursor.row), Some(2));
    assert!(window.copy(Direction::RightToLeft));
    assert_eq!(text(&window, CompareSide::Left), "a\nX\nY\nd\n");
    recompute(&mut window);
    assert!(window.undo_redo(true));
    assert_eq!(text(&window, CompareSide::Left), "a\nb\nc\nd\n");
    recompute(&mut window);
    assert!(window.undo_redo(false));
    recompute(&mut window);
    assert_eq!(window.changes, 0);
}

#[test]
fn insert_adds_a_line_before_the_cursor_and_delete_removes_it() {
    let mut window = window("a\nc\n", "a\nb\nc\n");
    window.toggle_edit();
    // The row of `b`, which the left side lacks: Enter there inserts.
    assert_eq!(window.edit.map(|cursor| cursor.row), Some(1));
    assert!(window.start_field(false));
    assert!(window.field.as_ref().is_some_and(|field| field.insert));
    window.type_field("b".into());
    assert!(window.commit_field());
    assert_eq!(text(&window, CompareSide::Left), "a\nb\nc\n");
    recompute(&mut window);
    window.set_side(CompareSide::Right);
    assert!(window.delete_line());
    assert_eq!(text(&window, CompareSide::Right), "a\nc\n");
    recompute(&mut window);
    assert!(window.start_field(true));
    assert!(window.cancel_field());
    assert!(window.field.is_none());
}

#[test]
fn saves_cover_the_changed_sides_and_their_answers_clean_them() {
    let mut window = window("a\n", "b\n");
    window.toggle_edit();
    assert!(window.copy(Direction::LeftToRight));
    recompute(&mut window);
    window.then = Some(Then::Close);
    let calls = window.start_saves(None, false);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].side, CompareSide::Right);
    assert_eq!(calls[0].bytes.as_slice(), b"a\n");
    assert!(window.fresh().is_none(), "no change while it saves");
    let origin = calls[0].origin.clone();
    assert_eq!(
        window.saved(CompareSide::Right, Ok(origin)),
        Some(Then::Close)
    );
    assert!(!window.dirty());
}

#[test]
fn a_file_changed_on_disk_asks_and_a_failure_is_named() {
    let mut window = window("a\n", "b\n");
    window.toggle_edit();
    assert!(window.copy(Direction::RightToLeft));
    recompute(&mut window);
    assert_eq!(window.start_saves(None, false).len(), 1);
    assert_eq!(
        window.saved(CompareSide::Left, Err("compare_changed")),
        None
    );
    assert_eq!(window.ask, Some(Ask::Changed(CompareSide::Left)));
    window.ask = None;
    assert_eq!(window.start_saves(Some(CompareSide::Left), true).len(), 1);
    assert_eq!(
        window.saved(CompareSide::Left, Err("permission_denied")),
        None
    );
    assert_eq!(window.note, Some(Note::Failed("permission_denied")));
    assert!(window.dirty());
}

#[test]
fn a_read_only_side_is_named_and_not_changed() {
    let mut right = side("b\n", "/r.txt");
    right.read_only = Some(rc_engine::compare::ReadOnly::Archive);
    let session = EditSession::new(side("a\n", "/l.txt"), right);
    let (l, r) = session.files();
    let diff = TextDiff::compute(l, r, &TextOptions::default());
    let mut window = DiffWindow::new(VfsPath::local("/l.txt"), VfsPath::local("/r.txt"), 1);
    window.loaded(Ok(Loaded::Text(Arc::new(diff), Box::new(session))));
    window.toggle_edit();
    assert_eq!(
        window.note,
        Some(Note::ReadOnly(
            CompareSide::Right,
            rc_engine::compare::ReadOnly::Archive
        ))
    );
    assert!(!window.copy(Direction::LeftToRight));
    window.set_side(CompareSide::Right);
    assert!(!window.start_field(false));
}
