use super::*;
use crate::compare::{TextOptions, text::decode};

fn side(content: &str, name: &str) -> SideFile {
    SideFile {
        file: decode(content.as_bytes()).expect("text"),
        origin: Origin {
            path: VfsPath::local(name),
            size: content.len() as u64,
            modified: None,
        },
        read_only: None,
    }
}

fn session(left: &str, right: &str) -> EditSession {
    EditSession::new(side(left, "/l.txt"), side(right, "/r.txt"))
}

/// Both sides as text, as they would be written.
fn texts(session: &EditSession) -> (String, String) {
    let text = |side| String::from_utf8(session.encoded(side).expect("encodes")).expect("UTF-8");
    (text(CompareSide::Left), text(CompareSide::Right))
}

fn diff(session: &EditSession) -> TextDiff {
    let (left, right) = session.files();
    TextDiff::compute(left, right, &TextOptions::default())
}

#[test]
fn a_line_is_changed_inserted_and_deleted_keeping_how_the_file_ends() {
    let mut edits = session("a\nb\n", "a\nb");
    assert!(edits.set_line(CompareSide::Left, 1, "B".into()));
    assert_eq!(texts(&edits).0, "a\nB\n");
    assert!(edits.insert_line(CompareSide::Left, 0, "zero".into()));
    assert_eq!(texts(&edits).0, "zero\na\nB\n");
    assert!(edits.delete_line(CompareSide::Left, 1));
    assert_eq!(texts(&edits).0, "zero\nB\n");
    // The right file has no ending after its last line, and keeps it so.
    assert!(edits.insert_line(CompareSide::Right, 2, "c".into()));
    assert_eq!(texts(&edits).1, "a\nb\nc");
    assert!(edits.delete_line(CompareSide::Right, 2));
    assert_eq!(texts(&edits).1, "a\nb");
    assert!(
        !edits.set_line(CompareSide::Right, 9, "x".into()),
        "no line 9"
    );
    assert!(edits.dirty(CompareSide::Left));
}

#[test]
fn new_lines_take_the_ending_most_lines_have() {
    let mut edits = session("a\r\nb\r\nc\n", "x\n");
    assert!(edits.insert_line(CompareSide::Left, 1, "new".into()));
    assert_eq!(texts(&edits).0, "a\r\nnew\r\nb\r\nc\n");
}

#[test]
fn a_block_is_copied_both_ways_and_the_diff_follows() {
    let mut edits = session("a\nb\nc\nd\n", "a\nX\nc\nd\nY\n");
    let before = diff(&edits);
    let blocks = blocks(&before);
    assert_eq!(blocks, [1..2, 4..5]);
    assert!(edits.copy_rows(&before, blocks[0].clone(), Direction::LeftToRight));
    assert_eq!(texts(&edits).1, "a\nb\nc\nd\nY\n");
    let after = diff(&edits);
    assert_eq!(after.changes(), 1, "only the inserted Y is left");
    assert!(
        !edits.copy_rows(&before, 4..5, Direction::LeftToRight),
        "a diff of the text before the change copies nothing"
    );
    // Y is only on the right: copied to the left it is inserted there.
    assert!(edits.copy_rows(&after, rows_at(&after, 4), Direction::RightToLeft));
    assert_eq!(texts(&edits).0, "a\nb\nc\nd\nY\n");
    assert!(diff(&edits).is_equal());
    assert!(edits.dirty(CompareSide::Left) && edits.dirty(CompareSide::Right));
}

#[test]
fn a_block_only_on_the_left_deletes_its_lines_on_the_right_when_copied_from_the_right() {
    let mut edits = session("a\nold\nc\n", "a\nc\n");
    let shown = diff(&edits);
    assert!(edits.copy_rows(&shown, rows_at(&shown, 1), Direction::RightToLeft));
    assert_eq!(texts(&edits).0, "a\nc\n");
}

#[test]
fn several_blocks_are_copied_from_the_last_so_their_lines_hold() {
    let mut edits = session("1\nA\n3\nB\nC\n6\n", "1\n2\n3\n6\n");
    let shown = diff(&edits);
    let all = blocks(&shown);
    assert_eq!(all.len(), 2);
    assert_eq!(edits.copy_blocks(&shown, &all, Direction::LeftToRight), 2);
    assert_eq!(texts(&edits).1, "1\nA\n3\nB\nC\n6\n");
}

#[test]
fn undo_and_redo_walk_the_changes_and_the_result_is_the_computed_diff() {
    let mut edits = session("a\nb\n", "a\nc\n");
    let shown = diff(&edits);
    assert!(edits.copy_rows(&shown, 1..2, Direction::LeftToRight));
    assert!(edits.set_line(CompareSide::Left, 0, "A".into()));
    let generation = edits.generation();
    assert!(edits.undo());
    assert!(edits.generation() > generation);
    assert_eq!(texts(&edits), ("a\nb\n".into(), "a\nb\n".into()));
    assert!(edits.undo());
    assert_eq!(texts(&edits), ("a\nb\n".into(), "a\nc\n".into()));
    assert!(!edits.any_dirty());
    assert!(!edits.undo(), "nothing left to undo");
    assert!(edits.redo() && edits.redo());
    assert_eq!(texts(&edits), ("A\nb\n".into(), "a\nb\n".into()));
    assert!(!edits.redo());
    // What the window shows after the changes is the diff computed of the text now.
    let now = diff(&edits);
    assert!(edits.current(&now));
    assert_eq!(
        now,
        TextDiff::compute(
            decode(b"A\nb\n").expect("text"),
            decode(b"a\nb\n").expect("text"),
            &TextOptions::default(),
        )
    );
}

#[test]
fn a_change_after_undo_drops_the_redo_and_a_save_marks_the_side_clean() {
    let mut edits = session("a\n", "b\n");
    assert!(edits.set_line(CompareSide::Left, 0, "x".into()));
    let origin = edits.origin(CompareSide::Left).clone();
    edits.saved(CompareSide::Left, origin);
    assert!(!edits.dirty(CompareSide::Left));
    assert!(edits.undo());
    assert!(edits.dirty(CompareSide::Left), "the saved text was x");
    assert!(edits.set_line(CompareSide::Left, 0, "y".into()));
    assert!(!edits.can_redo());
    assert!(edits.dirty(CompareSide::Left));
    assert!(edits.undo());
    assert!(edits.dirty(CompareSide::Left), "x is no longer reachable");
}

#[test]
fn undo_goes_back_at_most_the_limit() {
    let mut edits = session("0\n", "0\n");
    for step in 1..=UNDO_LIMIT + 5 {
        assert!(edits.set_line(CompareSide::Left, 0, step.to_string()));
    }
    let mut undone = 0;
    while edits.undo() {
        undone += 1;
    }
    assert_eq!(undone, UNDO_LIMIT);
    assert_eq!(texts(&edits).0, "5\n");
}

#[test]
fn a_read_only_side_takes_no_change() {
    let mut right = side("b\n", "/r.txt");
    right.read_only = Some(ReadOnly::Archive);
    let mut edits = EditSession::new(side("a\n", "/l.txt"), right);
    let shown = diff(&edits);
    assert!(!edits.set_line(CompareSide::Right, 0, "x".into()));
    assert!(!edits.copy_rows(&shown, 0..1, Direction::LeftToRight));
    assert!(edits.copy_rows(&shown, 0..1, Direction::RightToLeft));
    assert_eq!(texts(&edits).0, "b\n");
}

#[test]
fn the_cursor_copies_its_block_or_its_own_equal_row() {
    let edits = session("a\nb\nc\nd\n", "a\nX\nY\nd\n");
    let shown = diff(&edits);
    assert_eq!(rows_at(&shown, 0), 0..1);
    assert_eq!(rows_at(&shown, 2), 1..3);
    assert_eq!(rows_at(&shown, 9), 9..9);
}
