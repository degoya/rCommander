use super::*;

fn place(id: &str) -> usize {
    MENUS
        .iter()
        .position(|menu| menu.id == id)
        .unwrap_or_else(|| panic!("no menu {id}"))
}

/// RC-100-86: the Start menu takes Total Commander's place right after Configuration, the
/// Workspaces follow it, Help stays last.
#[test]
fn start_follows_configuration_and_workspaces_follow_start() {
    assert_eq!(place("start"), place("configuration") + 1);
    assert_eq!(place("workspaces"), place("start") + 1);
    assert_eq!(place("help"), MENUS.len() - 1);
}

/// The Start menu opens with "Change Start menu..."; its entries follow when the menu is built.
#[test]
fn the_start_menu_starts_with_its_window_and_configuration_has_the_own_commands() {
    assert_eq!(MENUS[place("start")].slots, [Entry("start_menu.manager")]);
    assert!(
        MENUS[place("configuration")]
            .slots
            .contains(&Entry("usercmd.manager"))
    );
}
