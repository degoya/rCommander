use std::collections::HashSet;

use rc_engine::Chord;

use crate::app::connections::tree_edit::TreeAction;

/// The owner's toolbar (2026-11-01): rename, up, down, outdent, indent, star, delete - in this
/// order, each with an icon of its own and a key that the tree takes.
#[test]
fn the_toolbar_holds_the_owner_s_actions_each_with_its_icon_and_key() {
    use TreeAction::{Delete, Down, Indent, Outdent, Rename, Star, Up};
    assert_eq!(
        TreeAction::TOOLBAR,
        [Rename, Up, Down, Outdent, Indent, Star, Delete]
    );
    let icons: HashSet<char> = TreeAction::MENU
        .iter()
        .map(|action| action.icon().glyph())
        .collect();
    assert_eq!(icons.len(), TreeAction::MENU.len(), "an icon each");
    for action in TreeAction::MENU {
        assert!(
            action.keys().parse::<Chord>().is_ok(),
            "{action:?}: {}",
            action.keys()
        );
        assert!(!action.label().is_empty(), "{action:?}");
        assert!(
            !action.label().contains('&'),
            "{action:?}: no access key in a tooltip"
        );
    }
    // The menu is the toolbar and three more.
    assert_eq!(&TreeAction::MENU[..7], &TreeAction::TOOLBAR[..]);
}
