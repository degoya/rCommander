use rc_engine::{BarButton, BarItem, BarPlace, BarSpec, ButtonAction, CommandRef};

use super::{MenuKind, OpenMenu, item_rows, step};

fn button() -> BarItem {
    BarItem::Button(BarButton {
        action: ButtonAction::Command(CommandRef::default()),
        icon: None,
        tooltip: String::new(),
    })
}

#[test]
fn a_menu_lists_the_items_from_its_first_and_passes_separators() {
    let spec = BarSpec {
        items: vec![button(), BarItem::Separator, button(), button()],
        ..BarSpec::default()
    };
    let rows = item_rows(&spec, 1);
    assert_eq!(rows, [1, 2, 3], "the overflow from the first hidden one");
    assert_eq!(step(&spec, &item_rows(&spec, 0), 0, true), 2);
    assert_eq!(step(&spec, &item_rows(&spec, 0), 2, false), 0);
    assert_eq!(
        step(&spec, &item_rows(&spec, 0), 3, true),
        3,
        "the last stays"
    );
}

#[test]
fn a_menu_opens_unanchored_until_its_button_is_measured() {
    let mut menu = OpenMenu::new(MenuKind::Items { bar: 1, from: 0 }, BarPlace::Horizontal, 0);
    assert!(menu.under.is_none());
    menu.anchor(None);
    assert!(menu.under.is_some(), "under the bar's start without bounds");
    assert_eq!(menu.kind.bar(), 1);
}
