use iced::{Point, Rectangle};
use rc_engine::{
    Chord, Entry, EntryKind, ListMode, NameFormat, PanelId, PanelState, PanelView, SortOrder,
    VfsPath,
};
use rc_vfs::Attributes;

use super::{
    Choice, ContextMenu, MENU_WIDTH, MenuAction, Target, at_cursor, clicked, items, place,
    rename_chord,
};
use crate::{app::panel::Pane, theme::ROW_HEIGHT};

fn chord(keys: &str) -> Chord {
    keys.parse().expect("a chord")
}

/// `/data/work` with the files `a.txt`, `b.txt`, `c.txt` and `d.txt`, the cursor on `cursor`,
/// `selected` selected; a pane of `page` rows.
fn pane(cursor: usize, selected: &[&str], page: usize) -> Pane {
    let entries: Vec<Entry> = ["a.txt", "b.txt", "c.txt", "d.txt"]
        .iter()
        .map(|name| Entry {
            name: (*name).to_owned(),
            kind: EntryKind::File,
            size: 0,
            modified: None,
            created: None,
            attributes: Attributes::default(),
            hidden: false,
            owner: None,
        })
        .collect();
    let state = PanelState {
        path: VfsPath::local("/data/work"),
        entries: entries.into(),
        cursor,
        selection: selected.iter().map(|name| (*name).to_owned()).collect(),
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
    };
    let mut pane = Pane::new(state);
    pane.set_page_rows(page);
    pane
}

fn names(names: &[&str]) -> Target {
    Target::Entries(names.iter().map(|name| (*name).to_owned()).collect())
}

fn labels(target: &Target, over_shell: bool, bound: &[&str]) -> Vec<Chord> {
    let bound: Vec<Chord> = bound.iter().map(|keys| chord(keys)).collect();
    items(target, over_shell, false, |chord| {
        bound.contains(chord).then(|| chord.clone())
    })
    .into_iter()
    .filter_map(|item| item.key().cloned())
    .collect()
}

const ALL_KEYS: [&str; 10] = [
    "Enter",
    "F3",
    "F4",
    "F5",
    "F6",
    "Shift+F6",
    "F8",
    "Alt+Enter",
    "F7",
    "Ctrl+R",
];

#[test]
fn a_right_click_on_a_selected_entry_takes_the_whole_selection_in_listing_order() {
    // Row 0 is `..`; row 3 shows c.txt.
    let pane = pane(0, &["d.txt", "c.txt", "a.txt"], 10);
    assert_eq!(
        clicked(&pane, 3),
        (names(&["a.txt", "c.txt", "d.txt"]), false)
    );
}

#[test]
fn a_right_click_beside_the_selection_takes_that_entry_and_drops_the_selection() {
    let pane = pane(0, &["a.txt"], 10);
    assert_eq!(clicked(&pane, 2), (names(&["b.txt"]), true));
    let unselected = self::pane(0, &[], 10);
    assert_eq!(clicked(&unselected, 2), (names(&["b.txt"]), false));
}

#[test]
fn a_right_click_on_the_parent_row_or_past_the_entries_is_the_folder_s() {
    let pane = pane(0, &["a.txt"], 10);
    assert_eq!(clicked(&pane, 0), (Target::Folder, false));
    assert_eq!(clicked(&pane, 9), (Target::Folder, false));
}

#[test]
fn the_keyboard_takes_the_selection_else_the_cursor_s_entry() {
    assert_eq!(
        at_cursor(&pane(1, &["d.txt", "b.txt"], 10)),
        names(&["b.txt", "d.txt"])
    );
    assert_eq!(at_cursor(&pane(2, &[], 10)), names(&["c.txt"]));
}

#[test]
fn the_keyboard_on_the_parent_row_takes_the_folder() {
    let mut on_parent = pane(0, &[], 10);
    on_parent.point_at(0);
    assert!(on_parent.on_parent());
    assert_eq!(at_cursor(&on_parent), Target::Folder);
}

#[test]
fn rcommander_s_own_menu_offers_the_file_commands_whose_keys_are_bound() {
    let entries = names(&["a.txt"]);
    assert_eq!(
        labels(&entries, false, &ALL_KEYS),
        [
            "Enter",
            "F3",
            "F4",
            "F5",
            "F6",
            "Shift+F6",
            "F8",
            "Alt+Enter"
        ]
        .map(chord)
    );
    assert_eq!(
        labels(&entries, false, &["Enter", "F5"]),
        ["Enter", "F5"].map(chord)
    );
    assert_eq!(
        labels(&Target::Folder, false, &ALL_KEYS),
        ["F7", "Ctrl+R"].map(chord)
    );
}

/// RC-020-10: checksum files also get "Verify checksums", Ctrl+Shift+F10's check, after the file
/// commands - alone and above the Explorer's menu; never the folder, never while unbound.
#[test]
fn checksum_files_are_offered_their_check() {
    let sums = names(&["set.sfv"]);
    let mut keys = ALL_KEYS.to_vec();
    keys.push("Ctrl+Shift+F10");
    let with = |target: &Target, over_shell: bool, keys: &[&str]| -> Vec<Chord> {
        let bound: Vec<Chord> = keys.iter().map(|key| chord(key)).collect();
        items(target, over_shell, true, |chord| {
            bound.contains(chord).then(|| chord.clone())
        })
        .into_iter()
        .filter_map(|item| item.key().cloned())
        .collect()
    };
    let alone = with(&sums, false, &keys);
    assert_eq!(alone.last(), Some(&chord("Ctrl+Shift+F10")));
    assert_eq!(alone.len(), 9);
    assert_eq!(
        with(&sums, true, &keys),
        ["F5", "Shift+F6", "Alt+Enter", "Ctrl+Shift+F10"].map(chord)
    );
    assert_eq!(
        with(&sums, false, &ALL_KEYS),
        labels(&sums, false, &ALL_KEYS)
    );
    assert_eq!(
        with(&Target::Folder, false, &keys),
        ["F7", "Ctrl+R"].map(chord)
    );
    assert!(!labels(&sums, false, &keys).contains(&chord("Ctrl+Shift+F10")));
}

#[test]
fn above_the_explorer_s_menu_stand_copy_rename_and_rcommander_s_properties() {
    let entries = names(&["a.txt"]);
    assert_eq!(
        labels(&entries, true, &ALL_KEYS),
        ["F5", "Shift+F6", "Alt+Enter"].map(chord)
    );
    assert!(labels(&Target::Folder, true, &ALL_KEYS).is_empty());
    let bound: Vec<Chord> = ALL_KEYS.map(chord).to_vec();
    let over = items(&entries, true, false, |chord| {
        bound.contains(chord).then(|| chord.clone())
    });
    let alone = items(&entries, false, false, |chord| {
        bound.contains(chord).then(|| chord.clone())
    });
    let properties = |list: &[super::Item]| {
        list.iter()
            .find(|item| item.key() == Some(&chord("Alt+Enter")))
            .map(|item| item.label)
    };
    assert_ne!(properties(&over), properties(&alone), "whose properties");
}

#[test]
fn the_explorer_s_rename_is_shift_f6() {
    assert_eq!(rename_chord(), chord("Shift+F6"));
}

fn open(entries: usize) -> ContextMenu {
    let bound: Vec<Chord> = ALL_KEYS.map(chord).to_vec();
    let items: Vec<_> = items(&names(&["a.txt"]), false, false, |chord| {
        bound.contains(chord).then(|| chord.clone())
    })
    .into_iter()
    .take(entries)
    .collect();
    ContextMenu {
        panel: PanelId::Left,
        target: names(&["a.txt"]),
        row: 1,
        by_key: true,
        items,
        cursor: 0,
        list: None,
        shell: false,
    }
}

#[test]
fn the_arrows_walk_around_and_enter_presses_the_entry_s_key() {
    let mut menu = open(3);
    assert_eq!(menu.key(&chord("ArrowUp")), MenuAction::Stay);
    assert_eq!(menu.cursor, 2, "up from the first wraps to the last");
    assert_eq!(menu.key(&chord("ArrowDown")), MenuAction::Stay);
    assert_eq!(menu.cursor, 0, "down from the last wraps to the first");
    menu.key(&chord("End"));
    assert_eq!(
        menu.key(&chord("Enter")),
        MenuAction::Run(Choice::Key(chord("F4")))
    );
    menu.key(&chord("Home"));
    assert_eq!(
        menu.key(&chord("Space")),
        MenuAction::Run(Choice::Key(chord("Enter")))
    );
}

#[test]
fn escape_and_the_menu_keys_close_and_other_keys_are_swallowed() {
    let mut menu = open(3);
    for keys in ["Escape", "Shift+F10", "ContextMenu"] {
        assert_eq!(menu.key(&chord(keys)), MenuAction::Close, "{keys}");
    }
    for keys in ["F5", "a", "Ctrl+ArrowDown", "Tab"] {
        assert_eq!(menu.key(&chord(keys)), MenuAction::Stay, "{keys}");
    }
    assert_eq!(menu.cursor, 0);
}

fn list(width: f32, height: f32) -> Rectangle {
    Rectangle {
        x: 100.0,
        y: 50.0,
        width,
        height,
    }
}

#[test]
fn the_menu_opens_under_its_row() {
    let pane = pane(0, &[], 20);
    // Row 2 is the third line: below it is 2 + 3 rows.
    assert_eq!(
        place(3, &pane, 2, list(600.0, 480.0)),
        Point::new(124.0, 50.0 + 2.0 + 3.0 * ROW_HEIGHT)
    );
}

#[test]
fn without_room_below_the_menu_opens_above_its_row() {
    let pane = pane(0, &[], 20);
    let at = place(8, &pane, 4, list(600.0, 6.0 * ROW_HEIGHT));
    let height = 8.0 * ROW_HEIGHT + 8.0;
    assert_eq!(at.y, (50.0 + 2.0 + 4.0 * ROW_HEIGHT - height).max(50.0));
}

#[test]
fn a_narrow_list_keeps_the_menu_inside_and_the_keyboard_point_is_the_row_s_corner() {
    let pane = pane(0, &[], 20);
    assert_eq!(place(3, &pane, 0, list(MENU_WIDTH + 10.0, 480.0)).x, 110.0);
    assert_eq!(
        place(0, &pane, 1, list(600.0, 2.0 * ROW_HEIGHT)),
        Point::new(124.0, 50.0 + 2.0 + 2.0 * ROW_HEIGHT),
        "with no entries of its own nothing flips"
    );
}

/// In the brief view the menu opens in the row's column, as wide as the list's columns are:
/// three in 700 pixels, not the 200 they are counted by (RC-080-44).
#[test]
fn in_the_brief_view_the_menu_opens_in_the_row_s_column() {
    let mut pane = pane(0, &[], 2);
    pane.state.mode = ListMode::Brief;
    pane.set_width(700.0);
    assert_eq!(pane.columns(), 3);
    // Row 2 is the first of the second column.
    let at = place(3, &pane, 2, list(700.0, 480.0));
    assert_eq!(at.x, 100.0 + (24.0 + 700.0 / 3.0));
    assert_eq!(at.y, 50.0 + 2.0 + ROW_HEIGHT);
}

#[test]
fn entries_also_copy_their_names_and_full_paths_the_folder_does_not() {
    let bound: Vec<Chord> = ALL_KEYS.map(chord).to_vec();
    let copies = |target: &Target, over_shell: bool| -> Vec<Choice> {
        items(target, over_shell, false, |chord| {
            bound.contains(chord).then(|| chord.clone())
        })
        .into_iter()
        .filter(|item| item.key().is_none())
        .map(|item| item.choice)
        .collect()
    };
    let both = vec![
        Choice::CopyNames(NameFormat::Names),
        Choice::CopyNames(NameFormat::FullPaths),
    ];
    for over_shell in [true, false] {
        assert_eq!(copies(&names(&["a.txt"]), over_shell), both, "{over_shell}");
        assert!(copies(&Target::Folder, over_shell).is_empty());
    }
    // They need no key, and come after the keys' entries, right above the Explorer's.
    let alone = items(&names(&["a.txt"]), true, false, |_| None);
    assert_eq!(
        alone
            .into_iter()
            .map(|item| item.choice)
            .collect::<Vec<_>>(),
        both
    );
    let over = items(&names(&["a.txt"]), true, false, |chord| {
        bound.contains(chord).then(|| chord.clone())
    });
    let last: Vec<Choice> = over[over.len() - 2..]
        .iter()
        .map(|item| item.choice.clone())
        .collect();
    assert_eq!(last, both);
}

/// RC-080-16: a letter takes the one entry starting with it, as a Win32 menu does, and walks
/// several that start with it.
#[test]
fn a_letter_takes_its_entry_or_walks_those_starting_with_it() {
    let mut menu = open(10);
    assert_eq!(
        menu.key(&chord("v")),
        MenuAction::Run(Choice::Key(chord("F3")))
    );
    assert_eq!(
        menu.key(&chord("Shift+r")),
        MenuAction::Run(Choice::Key(chord("Shift+F6")))
    );
    // "Copy to...", "Copy name", "Copy full path".
    assert_eq!(menu.key(&chord("c")), MenuAction::Stay);
    assert_eq!(menu.cursor, 3);
    assert_eq!(menu.key(&chord("c")), MenuAction::Stay);
    assert_eq!(menu.cursor, 8);
    assert_eq!(menu.key(&chord("Ctrl+c")), MenuAction::Stay);
    assert_eq!(menu.cursor, 8, "Ctrl with a letter is no letter");
}

/// RC-100-21: an entry runs its command through the key the person moved it to and shows that
/// key; an entry whose command lost every key is left out.
#[test]
fn the_entries_follow_a_moved_key() {
    let mut keymap = rc_engine::Keymap::standard();
    keymap.unassign(&chord("F5")).expect("not locked");
    keymap.assign(chord("Ctrl+K"), "copy").expect("free");
    keymap.unassign(&chord("F6")).expect("not locked");
    let shown: Vec<Chord> = items(&names(&["a.txt"]), false, false, |chord| {
        keymap.moved_key(chord).cloned()
    })
    .into_iter()
    .filter_map(|item| item.key().cloned())
    .collect();
    assert!(shown.contains(&chord("Ctrl+K")), "{shown:?}");
    assert!(!shown.contains(&chord("F5")), "{shown:?}");
    assert!(!shown.contains(&chord("F6")), "move has no key: {shown:?}");
    assert!(
        shown.contains(&chord("F3")),
        "the others keep theirs: {shown:?}"
    );
}
