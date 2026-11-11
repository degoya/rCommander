use iced::{Point, Rectangle, Size};
use rc_engine::{BarPlace, Chord};

use super::{BarMenu, Entry, Level, MenuKey, SIZES, entries, items, spot};
use crate::app::button_bar::BarSettings;

fn chord(keys: &str) -> Chord {
    keys.parse().expect("a chord")
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rectangle {
    Rectangle::new(Point::new(x, y), Size::new(width, height))
}

/// RC-110-15: a button's menu offers Change, Delete and Insert before the bar's entries; beside
/// the buttons and in the vertical bar the bar's entries are the same; Symbol size lists Total
/// Commander's sizes and Other....
#[test]
fn the_entries_depend_on_where_the_menu_opens() {
    let bar_entries = [Entry::EditBar, Entry::IconSize, Entry::Flat, Entry::Hide];
    let on_button = entries(true, Level::Main);
    assert_eq!(
        &on_button[..3],
        [Entry::Change, Entry::Delete, Entry::Insert]
    );
    assert_eq!(&on_button[3..], bar_entries);
    assert_eq!(
        entries(false, Level::Main),
        bar_entries,
        "beside the buttons"
    );
    assert_eq!(
        entries(true, Level::Sizes),
        [
            Entry::Size(16),
            Entry::Size(24),
            Entry::Size(32),
            Entry::Size(48),
            Entry::OtherSize
        ]
    );
    let settings = BarSettings::default();
    let horizontal = items(BarPlace::Horizontal, false, Level::Main, &settings);
    let vertical = items(BarPlace::Vertical, false, Level::Main, &settings);
    let kinds = |items: &[super::Item]| items.iter().map(|item| item.entry).collect::<Vec<_>>();
    assert_eq!(kinds(&horizontal), kinds(&vertical), "the vertical bar's");
    let hide = |items: &[super::Item]| {
        items
            .iter()
            .find(|item| item.entry == Entry::Hide)
            .map(|item| item.label.clone())
    };
    assert_ne!(hide(&horizontal), hide(&vertical), "each hides its own bar");
    let vertical_button = items(BarPlace::Vertical, true, Level::Main, &settings);
    assert_eq!(kinds(&vertical_button), on_button);
    assert!(horizontal.iter().all(|item| !item.label.is_empty()));
    assert!(
        horizontal
            .iter()
            .any(|item| item.entry == Entry::IconSize && item.opens)
    );
}

/// The check marks follow the settings: flat buttons, the size in force, Other... for a size
/// the menu does not list.
#[test]
fn the_marks_follow_the_settings() {
    let checked = |settings: &BarSettings, level| {
        items(BarPlace::Horizontal, true, level, settings)
            .into_iter()
            .filter(|item| item.checked)
            .map(|item| item.entry)
            .collect::<Vec<_>>()
    };
    let standard = BarSettings::default();
    assert!(
        checked(&standard, Level::Main).is_empty(),
        "framed at first"
    );
    assert_eq!(checked(&standard, Level::Sizes), [Entry::Size(24)]);
    let flat = BarSettings {
        flat: true,
        icon_size: 40,
        ..BarSettings::default()
    };
    assert_eq!(checked(&flat, Level::Main), [Entry::Flat]);
    assert_eq!(checked(&flat, Level::Sizes), [Entry::OtherSize]);
}

/// The file list's keys: the arrows round the end, a letter takes its only entry, Symbol size
/// opens with Enter or Right and goes back with Escape or Left, Escape and Shift+F10 close.
#[test]
fn the_menu_takes_the_file_lists_keys() {
    let settings = BarSettings::default();
    let mut menu = BarMenu::new(BarPlace::Horizontal, 1, Some(0));
    let shown = menu.items(&settings);
    assert_eq!(menu.key(&chord("ArrowUp"), &shown), MenuKey::Stay);
    assert_eq!(menu.cursor, shown.len() - 1, "round the top");
    assert_eq!(menu.key(&chord("ArrowDown"), &shown), MenuKey::Stay);
    assert_eq!(menu.cursor, 0);
    assert_eq!(menu.key(&chord("End"), &shown), MenuKey::Stay);
    assert_eq!(menu.cursor, shown.len() - 1);
    // "Flat buttons" is the only entry with an F.
    let flat = shown
        .iter()
        .position(|item| item.entry == Entry::Flat)
        .expect("there");
    assert_eq!(menu.key(&chord("f"), &shown), MenuKey::Run(flat));
    let size = shown
        .iter()
        .position(|item| item.entry == Entry::IconSize)
        .expect("there");
    menu.cursor = size;
    assert_eq!(menu.key(&chord("ArrowRight"), &shown), MenuKey::Run(size));
    menu.open_sizes(&settings);
    assert_eq!(menu.level, Level::Sizes);
    assert_eq!(menu.cursor, 1, "on 24, the size in force");
    let sizes = menu.items(&settings);
    assert_eq!(sizes.len(), SIZES.len() + 1);
    assert_eq!(menu.key(&chord("Enter"), &sizes), MenuKey::Run(1));
    assert_eq!(menu.key(&chord("ArrowLeft"), &sizes), MenuKey::Stay);
    assert_eq!(menu.level, Level::Main);
    assert_eq!(menu.cursor, size, "back on Symbol size");
    menu.open_sizes(&settings);
    assert_eq!(menu.key(&chord("Escape"), &sizes), MenuKey::Stay);
    assert_eq!(menu.level, Level::Main, "Escape goes back one");
    let shown = menu.items(&settings);
    assert_eq!(menu.key(&chord("F5"), &shown), MenuKey::Stay, "swallowed");
    assert_eq!(menu.key(&chord("Shift+F10"), &shown), MenuKey::Close);
    assert_eq!(menu.key(&chord("Escape"), &shown), MenuKey::Close);
    assert_eq!(menu.key(&chord("ContextMenu"), &shown), MenuKey::Close);
}

/// Under the button above the panels, beside it between them, at the bar's start for a click
/// beside the buttons; never past the bar's end.
#[test]
fn the_menu_stands_by_its_button_inside_the_bar() {
    let bar = rect(0.0, 30.0, 1000.0, 36.0);
    let button = rect(100.0, 32.0, 32.0, 32.0);
    assert_eq!(
        spot(BarPlace::Horizontal, bar, Some(button)),
        Point::new(100.0, 66.0)
    );
    assert_eq!(spot(BarPlace::Horizontal, bar, None), Point::new(8.0, 68.0));
    let far = rect(990.0, 32.0, 32.0, 32.0);
    let at = spot(BarPlace::Horizontal, bar, Some(far));
    assert!(at.x + super::MENU_WIDTH <= bar.x + bar.width, "{at:?}");
    let vertical = rect(500.0, 60.0, 36.0, 600.0);
    let button = rect(502.0, 100.0, 32.0, 32.0);
    assert_eq!(
        spot(BarPlace::Vertical, vertical, Some(button)),
        Point::new(538.0, 100.0)
    );
    let low = rect(502.0, 640.0, 32.0, 32.0);
    let at = spot(BarPlace::Vertical, vertical, Some(low));
    assert!(at.y < 640.0, "lifted to stay in the bar: {at:?}");
    assert!(at.y >= vertical.y);
}
