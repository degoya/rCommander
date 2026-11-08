use super::*;

fn menu(items: Vec<BarItem>) -> BarMenu {
    BarMenu {
        title: "&Start".into(),
        items,
    }
}

fn entry(id: &'static str, text: &str) -> BarItem {
    BarItem::Entry {
        id,
        text: text.into(),
    }
}

/// The Start menu is filled again on every change: a title changes under the same id.
#[test]
fn a_menu_with_start_entries_or_submenus_is_always_filled_again() {
    assert!(!always(&menu(vec![entry("workspace:1", "&1 Web")])));
    assert!(!always(&menu(vec![
        entry("start_menu.manager", "&Change"),
        BarItem::Separator
    ])));
    assert!(always(&menu(vec![entry("start:4", "Putty")])));
    assert!(always(&menu(vec![BarItem::Submenu {
        text: "Tools".into(),
        items: Vec::new(),
    }])));
}

/// A key changed in the keymap reaches an entry inside a submenu too.
#[test]
fn an_entry_is_found_in_its_submenu() {
    let items = vec![
        entry("start:1", "Notepad"),
        BarItem::Submenu {
            text: "Tools".into(),
            items: vec![entry("start:4", "Putty\tCtrl+Alt+P")],
        },
    ];
    assert_eq!(text_of(&items, "start:4"), Some("Putty\tCtrl+Alt+P"));
    assert_eq!(text_of(&items, "start:1"), Some("Notepad"));
    assert_eq!(text_of(&items, "start:9"), None);
}
