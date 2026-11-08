use rc_engine::{CommandRef, StartItem, StartItemKind, UserCommand};

use super::*;

fn command(id: i64, name: &str, tooltip: &str) -> UserCommand {
    UserCommand {
        id,
        name: name.into(),
        action: CommandRef::default(),
        tooltip: tooltip.into(),
        icon_path: String::new(),
        icon_index: 0,
    }
}

fn row(id: i64, kind: StartItemKind, title: &str) -> StartItem {
    StartItem {
        id,
        parent: None,
        depth: 0,
        kind,
        title: title.into(),
        action: None,
    }
}

#[test]
fn the_own_commands_are_offered_by_their_tooltip_and_bound_by_their_name() {
    let commands = [command(1, "em_np", "Notepad"), command(2, "em_putty", "")];
    let offered = options(CommandKeyKind::Own, &commands, &[]);
    let names: Vec<&str> = offered.iter().map(|option| option.name.as_str()).collect();
    assert_eq!(names, ["em_np", "em_putty"]);
    assert_eq!(offered[0].label, command_label("Notepad"));
    assert_eq!(offered[1].label, command_label("em_putty"));
}

#[test]
fn only_the_start_menus_entries_are_offered_as_start_keys() {
    let items = [
        row(1, StartItemKind::Item, "&Notepad"),
        row(2, StartItemKind::Submenu, "Tools"),
        row(3, StartItemKind::Separator, ""),
        row(4, StartItemKind::Item, "Putty"),
    ];
    let offered = options(CommandKeyKind::Start, &[], &items);
    let names: Vec<&str> = offered.iter().map(|option| option.name.as_str()).collect();
    assert_eq!(names, ["start:1", "start:4"]);
    assert_eq!(offered[0].label, start_label("&Notepad"));
}

#[test]
fn a_keys_name_says_what_it_is_for() {
    assert_eq!(kind_of("em_np"), CommandKeyKind::Own);
    assert_eq!(kind_of("start:4"), CommandKeyKind::Start);
}
