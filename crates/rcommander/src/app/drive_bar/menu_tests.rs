use rc_engine::{Chord, Drive, DriveGroup, DriveKind, DriveState, PanelId, VfsPath};

use super::{DriveMenu, MenuAction, MenuLine, Scope, cursor_line, lines};
use crate::app::drive_bar::actions::DriveAction;

fn entry(id: &str, group: DriveGroup, label: &str) -> Drive {
    Drive {
        id: id.to_owned(),
        group,
        kind: DriveKind::Fixed,
        label: label.to_owned(),
        root: Some(VfsPath::local(format!("/{label}"))),
        state: DriveState::Ready,
        volume: None,
        space: None,
        folder: Vec::new(),
        favourite: false,
        folder_buttons: Vec::new(),
        remote: None,
    }
}

/// `C:`, `D:`, then the distributions `Debian` and `Ubuntu`, then the server `web`.
fn drives() -> Vec<Drive> {
    vec![
        entry("C:", DriveGroup::Drives, "C:"),
        entry("D:", DriveGroup::Drives, "D:"),
        entry("wsl:Debian", DriveGroup::Wsl, "Debian"),
        entry("wsl:Ubuntu", DriveGroup::Wsl, "Ubuntu"),
        entry("server:1", DriveGroup::Servers, "web"),
    ]
}

fn key(menu: &mut DriveMenu, keys: &str) -> MenuAction {
    let chord: Chord = keys.parse().expect("a chord");
    menu.key(&chord, &drives())
}

#[test]
fn the_menu_opens_on_the_panel_s_drive() {
    let menu = DriveMenu::new(PanelId::Right, &drives(), Some("wsl:Ubuntu"));
    assert_eq!(menu.cursor, 3);
    let unknown = DriveMenu::new(PanelId::Left, &drives(), Some("Z:"));
    assert_eq!(unknown.cursor, 0);
}

#[test]
fn arrows_home_and_end_move_and_enter_takes_the_entry() {
    let mut menu = DriveMenu::new(PanelId::Left, &drives(), None);
    assert_eq!(key(&mut menu, "ArrowUp"), MenuAction::Stay);
    assert_eq!(menu.cursor, 0, "the first entry is the top");
    key(&mut menu, "ArrowDown");
    key(&mut menu, "ArrowDown");
    assert_eq!(menu.cursor, 2);
    key(&mut menu, "End");
    key(&mut menu, "ArrowDown");
    assert_eq!(menu.cursor, 4, "the last entry is the bottom");
    key(&mut menu, "Home");
    assert_eq!(menu.cursor, 0);
    key(&mut menu, "ArrowDown");
    assert_eq!(key(&mut menu, "Enter"), MenuAction::Pick("D:".into()));
    assert_eq!(key(&mut menu, "Space"), MenuAction::Pick("D:".into()));
}

#[test]
fn a_drive_letter_takes_its_drive_at_once() {
    let mut menu = DriveMenu::new(PanelId::Left, &drives(), None);
    assert_eq!(key(&mut menu, "d"), MenuAction::Pick("D:".into()));
    assert_eq!(key(&mut menu, "Shift+C"), MenuAction::Pick("C:".into()));
}

#[test]
fn another_letter_moves_to_the_next_entry_starting_with_it() {
    let mut menu = DriveMenu::new(PanelId::Left, &drives(), None);
    assert_eq!(key(&mut menu, "u"), MenuAction::Stay);
    assert_eq!(menu.cursor, 3);
    key(&mut menu, "w");
    assert_eq!(menu.cursor, 4);
    key(&mut menu, "x");
    assert_eq!(menu.cursor, 4, "no entry starts with x");
}

#[test]
fn escape_closes_and_alt_f1_alt_f2_toggle_or_switch_panels() {
    let mut menu = DriveMenu::new(PanelId::Left, &drives(), None);
    assert_eq!(key(&mut menu, "Escape"), MenuAction::Close);
    assert_eq!(key(&mut menu, "Alt+F1"), MenuAction::Close);
    assert_eq!(key(&mut menu, "Alt+F2"), MenuAction::Switch(PanelId::Right));
    assert_eq!(key(&mut menu, "Ctrl+d"), MenuAction::Stay, "not a letter");
    assert_eq!(key(&mut menu, "Tab"), MenuAction::Stay);
}

#[test]
fn the_cursor_stays_on_an_entry_when_the_list_shrinks() {
    let mut menu = DriveMenu::new(PanelId::Left, &drives(), Some("server:1"));
    menu.clamp(&drives()[..2]);
    assert_eq!(menu.cursor, 1);
    menu.clamp(&[]);
    assert_eq!(menu.cursor, 0);
}

/// An entry's line: its stop, its index among the drives, whether it stands in.
fn at(stop: usize, drive: usize, indent: bool) -> MenuLine<'static> {
    MenuLine::Entry {
        stop,
        drive,
        indent,
    }
}

/// The lines as drawn, headings included: what the menu scrolls by to keep its cursor in view.
#[test]
fn the_cursor_s_line_counts_the_headings_above_it() {
    let mut drives = drives();
    for id in ["server:2", "server:3"] {
        let mut server = entry(id, DriveGroup::Servers, id);
        server.folder = vec!["Work".to_owned()];
        drives.push(server);
    }
    let work = ["Work".to_owned()];
    let mut menu = DriveMenu::new(PanelId::Left, &drives, None);
    assert_eq!(
        lines(&drives, &menu),
        [
            MenuLine::Group(DriveGroup::Drives),
            at(0, 0, false),
            at(1, 1, false),
            MenuLine::Group(DriveGroup::Wsl),
            at(2, 2, false),
            at(3, 3, false),
            MenuLine::Group(DriveGroup::Servers),
            at(4, 4, false),
            MenuLine::Folder(&work),
            at(5, 5, true),
            at(6, 6, true),
        ]
    );
    assert_eq!(cursor_line(&drives, &menu), (1, 11));
    menu.cursor = 6;
    assert_eq!(cursor_line(&drives, &menu), (10, 11), "the last line");
    assert_eq!(cursor_line(&[], &menu), (0, 0));
}

/// The network drive actions stand under the drives, under their own heading, and the arrows
/// reach them like a drive.
#[test]
fn the_actions_follow_the_drives_and_enter_runs_one() {
    let actions = vec![DriveAction::MapNetwork, DriveAction::UnmapNetwork];
    let drives = drives();
    let mut menu = DriveMenu::new(PanelId::Left, &drives, None).with_actions(actions);
    let drawn = lines(&drives, &menu);
    assert_eq!(
        drawn[drawn.len() - 3..],
        [
            MenuLine::Actions,
            MenuLine::Action(5, DriveAction::MapNetwork),
            MenuLine::Action(6, DriveAction::UnmapNetwork),
        ]
    );
    menu.cursor = 6;
    assert_eq!(cursor_line(&drives, &menu), (drawn.len() - 1, drawn.len()));

    key(&mut menu, "Home");
    key(&mut menu, "End");
    assert_eq!(menu.cursor, 6);
    assert_eq!(
        key(&mut menu, "Enter"),
        MenuAction::Run(DriveAction::UnmapNetwork)
    );
    key(&mut menu, "ArrowUp");
    assert_eq!(
        key(&mut menu, "Space"),
        MenuAction::Run(DriveAction::MapNetwork)
    );
    // A drive letter still takes its drive.
    assert_eq!(key(&mut menu, "c"), MenuAction::Pick("C:".into()));
    menu.clamp(&drives[..1]);
    assert_eq!(menu.cursor, 2, "one drive and the two actions");
}

/// The servers of a connection tree: `web` marked as a button of its own, `a` and `b` in the
/// folder `Work`, `c` in its subfolder `Staging`, `x` in the folder `Home`.
fn tree() -> Vec<Drive> {
    let mut drives = drives();
    for (id, folder) in [
        ("server:a", &["Work"][..]),
        ("server:b", &["Work"][..]),
        ("server:c", &["Work", "Staging"][..]),
        ("server:x", &["Home"][..]),
    ] {
        let mut server = entry(id, DriveGroup::Servers, id);
        server.folder = folder.iter().map(|name| (*name).to_owned()).collect();
        server.folder_buttons = vec![1];
        drives.push(server);
    }
    drives[4].favourite = true;
    drives
}

/// A folder's button lists its servers only: those right in it plain, a subfolder's under the
/// subfolder's heading, standing in; no group heading, no actions.
#[test]
fn a_folder_s_menu_lists_its_servers_and_its_subfolders_indented() {
    let drives = tree();
    let staging = ["Staging".to_owned()];
    let menu = DriveMenu::scoped(
        PanelId::Left,
        Scope::Folder(vec!["Work".into()]),
        &drives,
        None,
    )
    .with_actions(vec![DriveAction::MapNetwork]);
    assert_eq!(
        lines(&drives, &menu),
        [
            at(0, 5, false),
            at(1, 6, false),
            MenuLine::Folder(&staging),
            at(2, 7, true),
        ]
    );
    assert_eq!(menu.cursor, 0, "on the first entry");
    let on_current = DriveMenu::scoped(
        PanelId::Left,
        Scope::Folder(vec!["Work".into()]),
        &drives,
        Some("server:c"),
    );
    assert_eq!(on_current.cursor, 2, "on the panel's drive, listed");
    let elsewhere = DriveMenu::scoped(
        PanelId::Left,
        Scope::Folder(vec!["Work".into()]),
        &drives,
        Some("C:"),
    );
    assert_eq!(elsewhere.cursor, 0, "the panel's drive is not listed");
}

/// A marked subfolder's button (RC-100-63) lists the servers in it and below, nothing of its
/// parent; `»` heads it with its own name, not its path.
#[test]
fn a_subfolder_s_button_lists_only_its_own_servers() {
    let mut drives = tree();
    // `c` sits in Work / Staging, which is marked as well.
    drives[7].folder_buttons = vec![1, 2];
    let path = vec!["Work".to_owned(), "Staging".to_owned()];
    let menu = DriveMenu::scoped(PanelId::Left, Scope::Folder(path), &drives, None);
    assert_eq!(lines(&drives, &menu), [at(0, 7, false)]);

    let staging = ["Staging".to_owned()];
    let items = crate::app::drive_bar::overflow::bar_items(&drives);
    let at_staging = items
        .iter()
        .position(|item| {
            matches!(item, crate::app::drive_bar::overflow::BarItem::Folder(path) if path.len() == 2)
        })
        .expect("a button for Work / Staging");
    let hidden = DriveMenu::scoped(PanelId::Left, Scope::Overflow(at_staging), &drives, None);
    assert_eq!(lines(&drives, &hidden)[0], MenuLine::Folder(&staging));
}

/// `»` lists the bar's items from the first hidden one on, in the bar's order: a drive plain, a
/// folder as a heading over all its servers.
#[test]
fn the_overflow_menu_lists_the_hidden_items_in_the_bar_s_order() {
    let drives = tree();
    // The bar: C, D, gap, Debian, Ubuntu, gap, web, Work, Home.
    let work = ["Work".to_owned()];
    let staging = ["Work".to_owned(), "Staging".to_owned()];
    let home = ["Home".to_owned()];
    let menu = DriveMenu::scoped(
        PanelId::Right,
        Scope::Overflow(4),
        &drives,
        Some("server:b"),
    );
    assert_eq!(
        lines(&drives, &menu),
        [
            at(0, 3, false),
            at(1, 4, false),
            MenuLine::Folder(&work),
            at(2, 5, true),
            at(3, 6, true),
            MenuLine::Folder(&staging),
            at(4, 7, true),
            MenuLine::Folder(&home),
            at(5, 8, true),
        ]
    );
    assert_eq!(menu.cursor, 3, "on the panel's drive");
    let none = DriveMenu::scoped(PanelId::Right, Scope::Overflow(9), &drives, None);
    assert!(lines(&drives, &none).is_empty(), "nothing hidden");
}

/// The keys walk the list shown: the arrows stop at its ends, Enter takes its entry, a letter
/// finds only a listed entry and a drive letter not listed takes nothing.
#[test]
fn the_keys_work_on_the_scoped_list_only() {
    let drives = tree();
    let mut menu = DriveMenu::scoped(
        PanelId::Left,
        Scope::Folder(vec!["Work".into()]),
        &drives,
        None,
    );
    let press = |menu: &mut DriveMenu, keys: &str| {
        let chord: Chord = keys.parse().expect("a chord");
        menu.key(&chord, &drives)
    };
    press(&mut menu, "End");
    assert_eq!(menu.cursor, 2);
    press(&mut menu, "ArrowDown");
    assert_eq!(menu.cursor, 2, "the folder's last server is the bottom");
    assert_eq!(
        press(&mut menu, "Enter"),
        MenuAction::Pick("server:c".into())
    );
    assert_eq!(
        press(&mut menu, "Shift+F10"),
        MenuAction::Context("server:c".into())
    );
    assert_eq!(press(&mut menu, "c"), MenuAction::Stay, "C: is not listed");
    press(&mut menu, "s");
    assert_eq!(menu.cursor, 0, "server:a, wrapping around");
    press(&mut menu, "u");
    assert_eq!(menu.cursor, 0, "Ubuntu is not listed");

    let mut overflow = DriveMenu::scoped(PanelId::Left, Scope::Overflow(4), &drives, None);
    assert_eq!(
        press(&mut overflow, "Enter"),
        MenuAction::Pick("wsl:Ubuntu".into())
    );
    press(&mut overflow, "End");
    assert_eq!(overflow.cursor, 5);
    assert_eq!(
        press(&mut overflow, "Enter"),
        MenuAction::Pick("server:x".into())
    );
    overflow.clamp(&drives[..5]);
    assert_eq!(overflow.cursor, 1, "Ubuntu and web are left");
}
