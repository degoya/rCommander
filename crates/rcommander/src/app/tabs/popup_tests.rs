use super::*;

fn key(text: &str) -> Chord {
    text.parse().expect("chord")
}

fn dir(name: &str) -> VfsPath {
    VfsPath::local(format!("/{name}"))
}

fn entry(id: i64, parent_id: Option<i64>, folder: bool) -> HotlistEntry {
    HotlistEntry {
        id,
        parent_id,
        name: format!("n{id}"),
        folder,
        path: (!folder).then(|| dir(&format!("p{id}"))),
        target: None,
    }
}

#[test]
fn the_history_lists_newest_first_with_the_cursor_on_the_folder_shown() {
    let mut popup = Popup::history(PanelId::Left, &[dir("a"), dir("b"), dir("c")], 1);
    let labels: Vec<&str> = popup.rows.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(labels, ["/c", "/b", "/a"]);
    assert_eq!(popup.cursor, 1);
    assert!(popup.rows[1].current);
    assert_eq!(popup.key(&key("ArrowDown")), PopupKey::Stay);
    assert_eq!(
        popup.key(&key("Enter")),
        PopupKey::Take(PopupAction::HistoryJump(0))
    );
    assert_eq!(popup.key(&key("Escape")), PopupKey::Close);
    assert_eq!(popup.key(&key("Alt+ArrowDown")), PopupKey::Close);
}

#[test]
fn hotlist_folders_are_headings_the_cursor_skips() {
    let items = [entry(1, None, true), entry(2, Some(1), false)];
    let mut popup = Popup::hotlist(PanelId::Right, &items);
    assert_eq!(popup.rows.len(), 4, "the items, add and configure");
    assert_eq!(popup.rows[1].depth, 1);
    assert_eq!(popup.cursor, 1, "the first entry, not the folder");
    assert_eq!(popup.key(&key("ArrowUp")), PopupKey::Stay);
    assert_eq!(popup.cursor, 1, "no row with an action above");
    assert_eq!(popup.take(0), PopupKey::Stay, "a heading takes nothing");
    assert_eq!(
        popup.key(&key("Enter")),
        PopupKey::Take(PopupAction::OpenHotlist(2))
    );
    popup.key(&key("End"));
    assert_eq!(
        popup.key(&key("Space")),
        PopupKey::Take(PopupAction::Configure)
    );
}

#[test]
fn an_empty_hotlist_still_offers_to_add_the_folder() {
    let popup = Popup::hotlist(PanelId::Left, &[]);
    assert_eq!(popup.rows.len(), 3);
    assert_eq!(popup.rows[0].action, None);
    assert_eq!(
        popup.take(popup.cursor),
        PopupKey::Take(PopupAction::AddCurrent)
    );
}

#[test]
fn a_tab_menu_offers_lock_copy_and_close() {
    let popup = Popup::tab_menu(PanelId::Left, TabId(4), true, |_| None);
    let actions: Vec<Option<PopupAction>> =
        popup.rows.iter().map(|row| row.action.clone()).collect();
    assert_eq!(
        actions,
        [
            Some(PopupAction::LockTab(TabId(4))),
            Some(PopupAction::CopyTab(TabId(4))),
            Some(PopupAction::CloseTab(TabId(4))),
        ]
    );
    assert!(popup.rows.iter().all(|row| row.key.is_none()));
}

/// Each entry asks for the key of the command it runs; one without a key shows none (RC-010-24).
#[test]
fn a_tab_menus_entries_show_their_commands_keys() {
    let popup = Popup::tab_menu(PanelId::Left, TabId(4), false, |command| {
        (command == "tabs.close").then(|| "Ctrl+W".to_owned())
    });
    let keys: Vec<Option<&str>> = popup.rows.iter().map(|row| row.key.as_deref()).collect();
    assert_eq!(keys, [None, None, Some("Ctrl+W")]);
}

fn tab(id: u64, title: &str, locked: bool, active: bool) -> TabView {
    TabView {
        id: TabId(id),
        title: title.to_owned(),
        path: dir(title),
        locked,
        active,
    }
}

#[test]
fn the_list_of_tabs_names_each_with_its_path_and_marks_the_locked_one() {
    let tabs = [
        tab(1, "src", false, false),
        tab(2, "docs", true, true),
        tab(3, "tmp", false, false),
    ];
    let popup = Popup::tabs(PanelId::Right, &tabs);
    assert_eq!(popup.panel, PanelId::Right);
    assert_eq!(popup.cursor, 1);
    let labels: Vec<&str> = popup.rows.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(labels, ["src", "*docs", "tmp"]);
    assert_eq!(popup.rows[2].detail, dir("tmp").path.display().to_string());
    assert!(popup.rows[1].current && !popup.rows[0].current);
    assert_eq!(
        popup.take(2),
        PopupKey::Take(PopupAction::SelectTab(TabId(3)))
    );
}

/// RC-080-16: in the list of tabs a letter takes the one tab starting with it (a locked tab's
/// `*` aside) or walks several, Delete closes the tab under the cursor, Shift+F10 and the
/// context-menu key open its menu.
#[test]
fn the_list_of_tabs_by_keyboard() {
    let tabs = [
        tab(1, "src", false, true),
        tab(2, "docs", true, false),
        tab(3, "spec", false, false),
    ];
    let mut popup = Popup::tabs(PanelId::Left, &tabs);
    assert_eq!(
        popup.key(&key("d")),
        PopupKey::Take(PopupAction::SelectTab(TabId(2)))
    );
    assert_eq!(popup.key(&key("s")), PopupKey::Stay);
    assert_eq!(popup.cursor, 2, "the next tab starting with s");
    assert_eq!(
        popup.key(&key("Delete")),
        PopupKey::Take(PopupAction::CloseTab(TabId(3)))
    );
    assert_eq!(popup.key(&key("Shift+F10")), PopupKey::TabMenu(TabId(3)));
    popup.key(&key("Home"));
    assert_eq!(popup.key(&key("ContextMenu")), PopupKey::TabMenu(TabId(1)));
    assert_eq!(popup.key(&key("x")), PopupKey::Stay);
}

/// RC-080-16: the list of a panel's terminal sessions starts on the one shown; Enter shows a
/// session, Delete closes it, a menu key does nothing.
#[test]
fn the_list_of_sessions_shows_and_closes_them() {
    let mut popup = Popup::sessions(
        PanelId::Right,
        [
            (TermId(7), "pwsh".to_owned(), false),
            (TermId(9), "bash".to_owned(), true),
        ],
    );
    assert_eq!((popup.panel, popup.cursor), (PanelId::Right, 1));
    assert!(popup.rows[1].current);
    assert_eq!(
        popup.key(&key("Delete")),
        PopupKey::Take(PopupAction::CloseTerminal(TermId(9)))
    );
    assert_eq!(popup.key(&key("Shift+F10")), PopupKey::Stay);
    popup.key(&key("ArrowUp"));
    assert_eq!(
        popup.key(&key("Enter")),
        PopupKey::Take(PopupAction::ShowTerminal(TermId(7)))
    );
    assert_eq!(
        popup.key(&key("b")),
        PopupKey::Take(PopupAction::ShowTerminal(TermId(9)))
    );
}

/// RC-010-24: Ctrl+Down and Ctrl+Up move the tab under the cursor one place in the list of tabs,
/// the cursor with it; at an end and in another popup nothing moves.
#[test]
fn ctrl_arrows_move_a_tab_in_the_list_of_tabs() {
    let tabs = [
        tab(1, "src", false, true),
        tab(2, "docs", false, false),
        tab(3, "tmp", false, false),
    ];
    let mut popup = Popup::tabs(PanelId::Left, &tabs);
    assert_eq!(popup.key(&key("Ctrl+ArrowUp")), PopupKey::Stay, "first");
    assert_eq!(
        popup.key(&key("Ctrl+ArrowDown")),
        PopupKey::MoveTab(TabId(1), 1)
    );
    assert_eq!(popup.cursor, 1);
    let labels: Vec<&str> = popup.rows.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(labels, ["docs", "src", "tmp"]);
    assert_eq!(
        popup.key(&key("Ctrl+ArrowDown")),
        PopupKey::MoveTab(TabId(1), 2)
    );
    assert_eq!(popup.key(&key("Ctrl+ArrowDown")), PopupKey::Stay, "last");
    assert_eq!(
        popup.key(&key("Ctrl+ArrowUp")),
        PopupKey::MoveTab(TabId(1), 1)
    );

    let mut history = Popup::history(PanelId::Left, &[dir("a"), dir("b")], 1);
    assert_eq!(history.key(&key("Ctrl+ArrowDown")), PopupKey::Stay);
    assert_eq!(history.cursor, 0);
}
