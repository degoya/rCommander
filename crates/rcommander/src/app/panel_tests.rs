use rc_engine::{
    Command, CursorStep, CursorTarget, Entry, EntryKind, ListMode, PanelState, PanelView,
    SortOrder, VfsPath,
};
use rc_vfs::Attributes;

use super::{BRIEF_COLUMN_WIDTH, Pane, Summary, ext, stem};

fn entry(name: &str, kind: EntryKind) -> Entry {
    Entry {
        name: name.to_owned(),
        kind,
        size: 0,
        modified: None,
        created: None,
        attributes: Attributes::default(),
        hidden: false,
        owner: None,
    }
}

/// The engine's state of `/data/work` with `count` files, the cursor on `cursor`.
fn state(count: usize, cursor: usize) -> PanelState {
    state_at("/data/work", count, cursor)
}

fn state_at(path: &str, count: usize, cursor: usize) -> PanelState {
    let entries: Vec<Entry> = (0..count)
        .map(|i| entry(&format!("file_{i:06}.txt"), EntryKind::File))
        .collect();
    PanelState {
        path: VfsPath::local(path),
        entries: entries.into(),
        cursor,
        selection: Default::default(),
        view: PanelView::Files,
        sort: SortOrder::default(),
        mode: ListMode::Full,
        show_hidden: false,
        remembered: Default::default(),
        filter: Default::default(),
        flat: None,
        archive: None,
        dir_sizes: Default::default(),
        listing: None,
    }
}

/// A pane over `count` entries with `page` rows on screen.
fn pane(count: usize, page: usize) -> Pane {
    let mut pane = Pane::new(state(count, 0));
    pane.set_page_rows(page);
    pane
}

#[test]
fn only_a_screen_of_rows_is_visible_in_a_huge_list() {
    let pane = pane(100_000, 40);
    assert_eq!(pane.rows(), 100_001, "the entries and `..`");
    assert_eq!(pane.visible_range(), 0..41);
}

#[test]
fn the_view_follows_the_engines_cursor() {
    let mut pane = pane(100_000, 30);
    pane.show(state(100_000, 99_999), false);
    assert_eq!(pane.cursor_row(), 100_000);
    assert_eq!(pane.top(), 100_001 - 30);
    assert_eq!(pane.visible_range(), 99_971..100_001);
    pane.show(state(100_000, 5), false);
    assert_eq!(pane.top(), 6);
    pane.show(state(100_000, 0), false);
    assert_eq!((pane.cursor_row(), pane.top()), (1, 1));
}

#[test]
fn the_view_moves_one_row_at_a_time_with_the_cursor() {
    let mut pane = pane(100, 5);
    pane.show(state(100, 5), false);
    assert_eq!(pane.top(), 2);
    pane.show(state(100, 4), false);
    assert_eq!(pane.top(), 2, "a cursor still in view scrolls nothing");
}

#[test]
fn the_wheel_scrolls_the_view_but_not_the_cursor() {
    let mut pane = pane(100, 10);
    pane.scroll(3);
    assert_eq!((pane.cursor_row(), pane.top()), (0, 3));
    pane.scroll(-10);
    assert_eq!(pane.top(), 0);
    pane.scroll(1_000);
    assert_eq!(pane.top(), 101 - 10);
}

/// RC-010-23: the scroll bar sets the view, never past the last page, and leaves the cursor.
#[test]
fn the_scroll_bar_sets_the_view_within_the_list() {
    let mut full = pane(100, 10);
    assert_eq!(full.max_top(), 101 - 10);
    full.scroll_to(50);
    assert_eq!((full.cursor_row(), full.top()), (0, 50));
    full.scroll_to(1_000);
    assert_eq!(full.top(), 91);
    let mut short = pane(5, 10);
    assert_eq!(short.max_top(), 0, "a list shorter than a page");
    short.scroll_to(3);
    assert_eq!(short.top(), 0);
    let mut columns = brief(95, 10, 3);
    assert_eq!(columns.max_top(), 70, "ten columns of 96 rows, three shown");
    columns.scroll_to(45);
    assert_eq!(columns.top(), 40, "the column the row falls in");
    columns.scroll_to(1_000);
    assert_eq!(columns.top(), 70);
    assert_eq!(brief(10, 20, 3).max_top(), 0, "one column of three");
}

#[test]
fn a_bigger_window_never_leaves_empty_space_at_the_end() {
    let mut pane = pane(100, 10);
    pane.show(state(100, 99), false);
    pane.set_page_rows(50);
    assert_eq!(pane.top(), 101 - 50);
}

#[test]
fn a_navigation_starts_at_the_top_and_drops_the_error() {
    let mut pane = pane(100, 10);
    pane.scroll(40);
    pane.error = Some("Permission denied.".to_owned());
    pane.show(state(100, 0), false);
    assert!(pane.error.is_some(), "a cursor change keeps the error");
    pane.show(state_at("/data/work/inner", 20, 0), true);
    assert_eq!((pane.top(), pane.error.as_deref()), (0, None));
}

#[test]
fn selected_rows_are_the_engines_selection_and_never_the_parent_row() {
    let mut selected = state(10, 0);
    selected.selection = ["file_000002.txt".to_owned(), "gone.txt".to_owned()].into();
    let pane = Pane::new(selected);
    assert!(pane.is_selected(3), "entry 2 sits under `..`");
    assert!(!pane.is_selected(0));
    assert!(!pane.is_selected(2));
    assert!(!pane.is_selected(99));
    assert_eq!(pane.selected_count(), 2);
}

#[test]
fn a_root_has_no_parent_row() {
    let pane = Pane::new(state_at("/", 3, 0));
    assert!(!pane.has_parent_row() && !pane.on_parent());
    assert_eq!((pane.rows(), pane.cursor_row()), (3, 0));
    assert_eq!(pane.entry_index(0), Some(0));
}

#[test]
fn an_empty_directory_shows_only_the_parent_row() {
    let mut pane = pane(0, 10);
    assert_eq!(pane.visible_range(), 0..1);
    assert!(pane.on_parent());
    assert_eq!(pane.through_parent_row(step(CursorStep::Down)), None);
    assert!(pane.on_parent(), "there is nothing below");
}

/// After entering a directory the cursor stands on `..`; after going up on the directory left.
#[test]
fn the_cursor_lands_on_the_parent_row_when_entering_only() {
    let mut pane = Pane::new(state_at("/data/work", 10, 0));
    pane.show(state_at("/data/work/inner", 10, 0), true);
    assert!(pane.on_parent());
    pane.show(state_at("/data/work", 10, 0), true);
    assert!(
        !pane.on_parent(),
        "back on the directory left, here the first"
    );
    assert_eq!(pane.cursor_row(), 1);
}

fn step(step: CursorStep) -> Command {
    Command::MoveCursor { panel: None, step }
}

#[test]
fn keys_cross_the_parent_row_in_the_window() {
    let mut pane = pane(100, 10);
    assert!(pane.on_parent());
    assert_eq!(pane.through_parent_row(step(CursorStep::Up)), None);
    assert_eq!(pane.through_parent_row(step(CursorStep::Down)), None);
    assert_eq!(
        pane.cursor_row(),
        1,
        "onto the first entry, the engine untouched"
    );
    assert_eq!(pane.through_parent_row(step(CursorStep::Up)), None);
    assert!(pane.on_parent());
    assert_eq!(
        pane.through_parent_row(step(CursorStep::PageDown)),
        Some(step(CursorStep::PageDown))
    );
    assert!(!pane.on_parent());
    pane.show(state(100, 40), false);
    assert_eq!(
        pane.through_parent_row(step(CursorStep::First)),
        Some(step(CursorStep::First))
    );
    assert!(pane.on_parent());
    pane.show(state(100, 0), false);
    assert!(pane.on_parent(), "the engine's first entry under `..`");
    pane.show(state(100, 5), false);
    assert_eq!(
        pane.through_parent_row(step(CursorStep::PageUp)),
        Some(step(CursorStep::First)),
        "a page up from near the top reaches `..`"
    );
    assert!(pane.on_parent());
}

#[test]
fn enter_on_the_parent_row_goes_up_and_insert_selects_nothing() {
    let mut pane = pane(10, 10);
    assert_eq!(
        pane.through_parent_row(Command::OpenEntry { panel: None }),
        Some(Command::Navigate {
            panel: None,
            path: "..".into()
        })
    );
    assert_eq!(
        pane.through_parent_row(Command::ToggleCursorSelection { panel: None }),
        None
    );
    assert_eq!(
        pane.cursor_row(),
        1,
        "Insert on `..` steps onto the first entry"
    );
    let insert = Command::ToggleCursorSelection { panel: None };
    assert_eq!(pane.through_parent_row(insert.clone()), Some(insert));
    let open = Command::OpenEntry { panel: None };
    assert_eq!(pane.through_parent_row(open.clone()), Some(open));
}

#[test]
fn a_click_on_the_parent_row_puts_the_engines_cursor_on_the_first_entry() {
    let mut pane = pane(10, 10);
    pane.show(state(10, 4), false);
    assert_eq!(pane.point_at(0), Some(0));
    assert!(pane.on_parent());
    assert_eq!(pane.point_at(3), Some(2));
    assert!(!pane.on_parent());
    let mut empty = pane_at_empty();
    assert_eq!(empty.point_at(0), None);
}

fn pane_at_empty() -> Pane {
    Pane::new(state_at("/data/work", 0, 0))
}

#[test]
fn ext_and_stem_split_a_file_at_the_last_dot() {
    let archive = entry("archive.tar.gz", EntryKind::File);
    assert_eq!((stem(&archive), ext(&archive)), ("archive.tar", "gz"));
}

#[test]
fn a_hidden_file_or_a_directory_has_no_extension() {
    let hidden = entry(".bashrc", EntryKind::File);
    assert_eq!((stem(&hidden), ext(&hidden)), (".bashrc", ""));
    let dir = entry("my.folder", EntryKind::Dir);
    assert_eq!((stem(&dir), ext(&dir)), ("my.folder", ""));
}

/// A brief pane over `count` entries: `rows` rows of `columns` columns.
fn brief(count: usize, rows: usize, columns: usize) -> Pane {
    let mut state = state(count, 0);
    state.mode = ListMode::Brief;
    let mut pane = Pane::new(state);
    pane.set_page_rows(rows);
    pane.set_width(columns as f32 * BRIEF_COLUMN_WIDTH + 10.0);
    pane
}

fn brief_state(count: usize, cursor: usize) -> PanelState {
    let mut state = state(count, cursor);
    state.mode = ListMode::Brief;
    state
}

#[test]
fn the_brief_view_shows_whole_columns_of_a_huge_list() {
    let mut pane = brief(100_000, 20, 3);
    assert_eq!((pane.columns(), pane.visible_range()), (3, 0..60));
    pane.show(brief_state(100_000, 70), false);
    assert_eq!(pane.cursor_row(), 71);
    assert_eq!(pane.top(), 20, "the cursor's column is the last one shown");
    assert_eq!(pane.visible_range(), 20..80);
    pane.show(brief_state(100_000, 99_999), false);
    assert_eq!(pane.top() % 20, 0, "a column starts the view");
    assert_eq!(pane.visible_range().end, 100_001);
    pane.scroll(-3);
    assert_eq!(pane.top(), 100_001_usize.div_ceil(20) * 20 - 80);
}

#[test]
fn the_full_view_has_one_column_whatever_the_width() {
    let mut pane = pane(100, 10);
    pane.set_width(5_000.0);
    assert_eq!((pane.columns(), pane.visible_range()), (1, 0..11));
}

fn key(step: CursorStep) -> Command {
    Command::MoveCursor { panel: None, step }
}

#[test]
fn left_and_right_move_by_a_column_in_the_brief_view_only() {
    let mut pane = brief(100, 10, 3);
    assert_eq!(
        pane.through_parent_row(key(CursorStep::Right)),
        Some(Command::SetCursor {
            panel: None,
            target: CursorTarget::Index(9)
        }),
        "from `..` (row 0) to row 10"
    );
    assert!(!pane.on_parent());
    pane.show(brief_state(100, 9), false);
    assert_eq!(
        pane.through_parent_row(key(CursorStep::Left)),
        Some(key(CursorStep::First))
    );
    assert!(pane.on_parent(), "back on `..`");
    assert_eq!(pane.through_parent_row(key(CursorStep::Left)), None);
    // `pane` the function, not the variable above.
    let mut full = self::pane(100, 10);
    assert_eq!(full.through_parent_row(key(CursorStep::Right)), None);
}

fn range(step: CursorStep) -> Command {
    Command::SelectRange { panel: None, step }
}

#[test]
fn shift_selection_never_takes_the_parent_row() {
    let mut pane = pane(10, 10);
    assert_eq!(pane.through_parent_row(range(CursorStep::Down)), None);
    assert_eq!(pane.cursor_row(), 1, "off `..`, nothing selected");
    assert_eq!(
        pane.through_parent_row(range(CursorStep::Up)),
        Some(range(CursorStep::Up))
    );
    assert!(
        pane.on_parent(),
        "the first entry is taken, the cursor on `..`"
    );
    assert_eq!(pane.through_parent_row(range(CursorStep::Up)), None);
    assert_eq!(
        pane.through_parent_row(range(CursorStep::PageDown)),
        Some(range(CursorStep::PageDown)),
        "a page from `..` starts at the first entry"
    );
    assert!(!pane.on_parent());
}

#[test]
fn enter_and_alt_num_plus_on_the_parent_row() {
    let mut pane = pane(10, 10);
    assert_eq!(
        pane.through_parent_row(Command::OpenFile {
            panel: None,
            path: None
        }),
        Some(Command::Navigate {
            panel: None,
            path: "..".into()
        })
    );
    let same = Command::SelectExtension {
        panel: None,
        select: true,
    };
    assert_eq!(pane.through_parent_row(same), None);
}

#[test]
fn the_status_sums_count_files_and_directories_apart() {
    let mut listed = state(0, 0);
    let sized = |name: &str, kind, size| {
        let mut entry = entry(name, kind);
        entry.size = size;
        entry
    };
    listed.entries = vec![
        sized("docs", EntryKind::Dir, 0),
        sized("src", EntryKind::Dir, 0),
        sized("a.bin", EntryKind::File, 1_000),
        sized("b.bin", EntryKind::File, 2_048),
        sized("c.bin", EntryKind::File, 5),
    ]
    .into();
    listed.selection = ["docs".to_owned(), "b.bin".to_owned(), "c.bin".to_owned()].into();
    let mut pane = Pane::new(listed.clone());
    assert_eq!(
        *pane.summary(),
        Summary {
            files: 3,
            dirs: 2,
            bytes: 3_053,
            selected_files: 2,
            selected_dirs: 1,
            selected_bytes: 2_053,
        }
    );
    listed.selection.clear();
    pane.show(listed, false);
    assert_eq!(
        (pane.summary().selected_files, pane.summary().selected_bytes),
        (0, 0)
    );
}

#[test]
fn leaving_a_flat_view_onto_a_file_puts_the_cursor_on_it_not_on_the_parent_row() {
    let mut pane = Pane::new(state_at("/data/work", 20, 0));
    pane.show(state_at("/data/work/inner/deeper", 10, 3), true);
    assert!(!pane.on_parent());
    assert_eq!(pane.cursor_row(), 4);
}

/// The engine's cursor put on the first entry (`panel.set_cursor` over MCP, say) is the window's
/// cursor too, not the `..` row; Home asks for `..` itself, and the engine's answer keeps it.
#[test]
fn the_engines_cursor_on_the_first_entry_leaves_the_parent_row_unless_asked() {
    let mut pane = Pane::new(state(10, 0));
    assert!(pane.on_parent());
    pane.show_cursor(state(10, 0));
    assert!(!pane.on_parent());
    assert_eq!(pane.cursor_row(), 1);

    let home = pane.through_parent_row(Command::MoveCursor {
        panel: None,
        step: CursorStep::First,
    });
    assert!(home.is_some());
    assert!(pane.on_parent());
    pane.show_cursor(state(10, 0));
    assert!(pane.on_parent(), "the answer to Home stays on ..");
    pane.show_cursor(state(10, 0));
    assert!(
        !pane.on_parent(),
        "a later move of the engine's own leaves it"
    );
}

/// What an operation takes (RC-080-38): the selection in the listing's order, else the cursor's
/// entry; on `..` without a selection nothing, not the first entry the engine's cursor is on.
#[test]
fn the_chosen_names_are_the_selection_else_the_cursor_and_none_on_the_parent_row() {
    let mut pane = Pane::new(state(5, 0));
    assert!(pane.on_parent());
    assert!(pane.chosen_names().is_empty());
    let mut selected = state(5, 0);
    selected.entries = selected
        .entries
        .iter()
        .rev()
        .cloned()
        .collect::<Vec<_>>()
        .into();
    selected.selection = ["file_000001.txt".to_owned(), "file_000003.txt".to_owned()].into();
    pane.show(selected, false);
    assert!(pane.on_parent());
    assert_eq!(pane.chosen_names(), ["file_000003.txt", "file_000001.txt"]);
    pane.show(state(5, 2), false);
    assert_eq!(pane.chosen_names(), ["file_000002.txt"]);
}
