use rc_engine::{Chord, Drive, DriveGroup, DriveKind, DriveState, PanelId, VfsPath};

use super::{DriveMenu, MenuAction, MenuLine, cursor_line, lines};
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
    menu.clamp(2);
    assert_eq!(menu.cursor, 1);
    menu.clamp(0);
    assert_eq!(menu.cursor, 0);
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
    assert_eq!(
        lines(&drives, &[]),
        [
            MenuLine::Group(DriveGroup::Drives),
            MenuLine::Entry(0),
            MenuLine::Entry(1),
            MenuLine::Group(DriveGroup::Wsl),
            MenuLine::Entry(2),
            MenuLine::Entry(3),
            MenuLine::Group(DriveGroup::Servers),
            MenuLine::Entry(4),
            MenuLine::Folder(&work),
            MenuLine::Entry(5),
            MenuLine::Entry(6),
        ]
    );
    assert_eq!(cursor_line(&drives, &[], 0), (1, 11));
    assert_eq!(cursor_line(&drives, &[], 6), (10, 11), "the last line");
    assert_eq!(cursor_line(&[], &[], 0), (0, 0));
}

/// The network drive actions stand under the drives, under their own heading, and the arrows
/// reach them like a drive.
#[test]
fn the_actions_follow_the_drives_and_enter_runs_one() {
    let actions = vec![DriveAction::MapNetwork, DriveAction::UnmapNetwork];
    let drives = drives();
    let drawn = lines(&drives, &actions);
    assert_eq!(
        drawn[drawn.len() - 3..],
        [
            MenuLine::Actions,
            MenuLine::Action(5, DriveAction::MapNetwork),
            MenuLine::Action(6, DriveAction::UnmapNetwork),
        ]
    );
    assert_eq!(
        cursor_line(&drives, &actions, 6),
        (drawn.len() - 1, drawn.len())
    );

    let mut menu = DriveMenu::new(PanelId::Left, &drives, None).with_actions(actions);
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
    menu.clamp(1);
    assert_eq!(menu.cursor, 2, "one drive and the two actions");
}
