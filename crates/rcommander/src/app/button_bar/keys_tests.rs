use rc_engine::{BarButton, BarItem, BarPlace, ButtonAction, Chord, CommandRef};

use super::{BarKey, first, key};

fn button() -> BarItem {
    BarItem::Button(BarButton {
        action: ButtonAction::Command(CommandRef::default()),
        icon: None,
        tooltip: String::new(),
    })
}

fn chord(keys: &str) -> Chord {
    keys.parse().expect("a chord")
}

fn items() -> Vec<BarItem> {
    vec![
        BarItem::Separator,
        button(),
        BarItem::Separator,
        button(),
        button(),
    ]
}

#[test]
fn the_arrows_along_the_bar_pass_separators() {
    let items = items();
    assert_eq!(first(&items), Some(1));
    let horizontal = |keys: &str, at| key(&chord(keys), BarPlace::Horizontal, &items, at);
    assert_eq!(horizontal("ArrowRight", 1), BarKey::Move(3));
    assert_eq!(horizontal("ArrowLeft", 3), BarKey::Move(1));
    assert_eq!(
        horizontal("ArrowLeft", 1),
        BarKey::Move(1),
        "the first stays"
    );
    assert_eq!(
        horizontal("ArrowRight", 4),
        BarKey::Move(4),
        "the last stays"
    );
    assert_eq!(horizontal("Home", 4), BarKey::Move(1));
    assert_eq!(horizontal("End", 1), BarKey::Move(4));
    let vertical = |keys: &str, at| key(&chord(keys), BarPlace::Vertical, &items, at);
    assert_eq!(vertical("ArrowDown", 1), BarKey::Move(3));
    assert_eq!(vertical("ArrowUp", 3), BarKey::Move(1));
}

#[test]
fn enter_runs_the_arrow_across_opens_escape_leaves() {
    let items = items();
    let horizontal = |keys: &str| key(&chord(keys), BarPlace::Horizontal, &items, 1);
    assert_eq!(horizontal("Enter"), BarKey::Press);
    assert_eq!(horizontal("ArrowDown"), BarKey::Open);
    assert_eq!(horizontal("Space"), BarKey::Open);
    assert_eq!(horizontal("Shift+F10"), BarKey::Context);
    assert_eq!(horizontal("Escape"), BarKey::Leave);
    assert_eq!(horizontal("Tab"), BarKey::Leave);
    assert_eq!(
        horizontal("F5"),
        BarKey::Stay,
        "a panel's key stays in the bar"
    );
    assert_eq!(horizontal("Ctrl+ArrowRight"), BarKey::Stay);
    let vertical = |keys: &str| key(&chord(keys), BarPlace::Vertical, &items, 1);
    assert_eq!(vertical("ArrowRight"), BarKey::Open);
}
