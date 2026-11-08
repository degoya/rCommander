use rc_engine::{BarButton, BarItem, BarSpec, ButtonAction, ButtonBar, CommandRef};

use super::{BarWindow, blank_button};

fn button(command: &str) -> BarItem {
    BarItem::Button(BarButton {
        action: ButtonAction::Command(CommandRef {
            command: command.into(),
            ..CommandRef::default()
        }),
        icon: None,
        tooltip: String::new(),
    })
}

fn bar(items: Vec<BarItem>) -> ButtonBar {
    ButtonBar {
        id: 7,
        name: "Tools".into(),
        spec: BarSpec {
            items,
            ..BarSpec::default()
        },
    }
}

#[test]
fn an_item_is_inserted_after_the_cursor() {
    let mut window = BarWindow::of(&bar(vec![button("a"), button("b")]));
    assert_eq!(window.cursor, Some(0));
    window.insert(BarItem::Separator);
    assert_eq!(window.cursor, Some(1));
    assert_eq!(window.items[1], BarItem::Separator);
    assert!(window.dirty);
    let mut empty = BarWindow::default();
    empty.insert(BarItem::Button(blank_button()));
    assert_eq!(empty.cursor, Some(0));
    assert_eq!(empty.spec().items.len(), 1);
}

#[test]
fn new_bars_leave_a_window_with_changes_alone() {
    let mut window = BarWindow::of(&bar(vec![button("a")]));
    window.refresh(&[bar(vec![button("a"), button("b")])]);
    assert_eq!(window.items.len(), 2, "read again");
    window.dirty = true;
    window.refresh(&[bar(Vec::new())]);
    assert_eq!(window.items.len(), 2, "changes stay");
    window.dirty = false;
    window.refresh(&[]);
    assert_eq!(window.bar, None, "the bar went: a new one");
}
