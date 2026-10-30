use rc_engine::{Chord, Keymap};

use super::*;
use crate::app::palette::{entries, reach};

fn menus() -> Vec<BarMenu> {
    build(
        &Keymap::standard(),
        &entries(&Keymap::standard(), &reach::standard(), false),
    )
}

fn chord(keys: &str) -> Chord {
    keys.parse().expect("a chord")
}

fn index(menus: &[BarMenu], title: &str) -> usize {
    menus
        .iter()
        .position(|menu| menu.title == title)
        .expect("the menu")
}

/// Alt with a title's access key names its menu - `o` of C&onfiguration too -, with Ctrl or
/// Shift or without Alt nothing.
#[test]
fn alt_with_a_titles_letter_names_its_menu() {
    let menus = menus();
    assert_eq!(
        menu_of(&menus, &chord("Alt+f")),
        Some(index(&menus, "&File"))
    );
    assert_eq!(
        menu_of(&menus, &chord("Alt+o")),
        Some(index(&menus, "C&onfiguration"))
    );
    for keys in ["f", "Ctrl+Alt+f", "Alt+Shift+f", "Alt+F5", "Alt+q"] {
        assert_eq!(menu_of(&menus, &chord(keys)), None, "{keys}");
    }
}

/// The arrows walk the entries over the separators and round, and the menus round; Home and
/// End go to the first and the last entry.
#[test]
fn the_arrows_walk_entries_and_menus_round() {
    let menus = menus();
    let file = index(&menus, "&File");
    let items = &menus[file].items;
    let mut open = OpenMenu {
        menu: file,
        cursor: first(&menus, file),
    };
    assert_eq!(open.cursor, Some(0));
    assert_eq!(key(&menus, &mut open, &chord("ArrowUp")), Step::Stay);
    assert_eq!(
        open.cursor,
        Some(items.len() - 1),
        "up from the first wraps"
    );
    assert_eq!(key(&menus, &mut open, &chord("Home")), Step::Stay);
    assert_eq!(open.cursor, Some(0));
    let separator = items
        .iter()
        .position(|item| *item == BarItem::Separator)
        .expect("File has a separator");
    open.cursor = Some(separator - 1);
    key(&menus, &mut open, &chord("ArrowDown"));
    assert_eq!(open.cursor, Some(separator + 1), "the separator is skipped");

    let last = menus.len() - 1;
    open = OpenMenu {
        menu: last,
        cursor: None,
    };
    key(&menus, &mut open, &chord("ArrowRight"));
    assert_eq!(
        open,
        OpenMenu {
            menu: 0,
            cursor: Some(0)
        }
    );
    key(&menus, &mut open, &chord("ArrowLeft"));
    assert_eq!(open.menu, last);
}

/// Enter takes the entry under the cursor, an entry's letter takes it at once, Escape closes;
/// Alt with another title's letter opens that menu.
#[test]
fn enter_or_a_letter_takes_an_entry_and_escape_closes() {
    let menus = menus();
    let help = index(&menus, "&Help");
    let mut open = OpenMenu {
        menu: help,
        cursor: first(&menus, help),
    };
    assert_eq!(
        key(&menus, &mut open, &chord("Enter")),
        Step::Choose("palette")
    );
    assert_eq!(key(&menus, &mut open, &chord("c")), Step::Choose("palette"));
    assert_eq!(key(&menus, &mut open, &chord("Escape")), Step::Close);
    assert_eq!(key(&menus, &mut open, &chord("Alt+f")), Step::Stay);
    assert_eq!(open.menu, index(&menus, "&File"));
}

#[cfg(not(windows))]
mod drawn {
    use super::*;
    use crate::app::{Message, harness::Harness, palette::PaletteMsg};

    fn bar(message: MenuBarMsg) -> Message {
        Message::Palette(PaletteMsg::MenuBar(message))
    }

    /// Shown, Alt+O opens Configuration and S its settings, the menu closed; while they are open
    /// Alt with a letter and a click on a title open nothing.
    #[tokio::test]
    async fn the_keyboard_opens_a_menu_and_takes_an_entry() {
        let (_root, mut harness) = Harness::temp_with_db().await;
        harness.send(bar(MenuBarMsg::Shown(true))).await;
        assert!(!harness.app.dialog_windows.menu_bar.menus.is_empty());
        harness.press("Alt+o").await;
        assert!(harness.app.dialog_windows.menu_bar.open.is_some());
        harness.press("s").await;
        assert!(harness.app.settings.is_some(), "Configuration > Settings");
        assert_eq!(harness.app.dialog_windows.menu_bar.open, None);

        harness.press("Alt+f").await;
        harness.send(bar(MenuBarMsg::Title(0))).await;
        assert_eq!(harness.app.dialog_windows.menu_bar.open, None);
    }

    /// In a terminal Alt with a title's letter is the shell's: Alt+V opens no menu (it is
    /// Claude Code's image paste).
    #[tokio::test]
    async fn alt_with_a_letter_in_a_terminal_opens_no_menu() {
        let (_root, mut harness) = Harness::temp_with_db().await;
        harness.send(bar(MenuBarMsg::Shown(true))).await;
        harness.press("Ctrl+[Semicolon]").await;
        assert_eq!(
            harness.app.pane(harness.app.active).state.view,
            rc_engine::PanelView::Terminal
        );
        harness.press("Alt+v").await;
        assert_eq!(harness.app.dialog_windows.menu_bar.open, None);
    }

    /// A click on a title opens its menu and a second one closes it; the pointer on another
    /// title moves there; F10 and Escape close the menu, they quit nothing.
    #[tokio::test]
    async fn the_pointer_opens_and_closes_a_menu() {
        let (_root, mut harness) = Harness::temp_with_db().await;
        harness.send(bar(MenuBarMsg::Shown(true))).await;
        harness.send(bar(MenuBarMsg::Title(0))).await;
        let open = |harness: &Harness| harness.app.dialog_windows.menu_bar.open;
        assert_eq!(
            open(&harness),
            Some(OpenMenu {
                menu: 0,
                cursor: None
            })
        );
        harness.send(bar(MenuBarMsg::HoverTitle(1))).await;
        assert_eq!(open(&harness).map(|open| open.menu), Some(1));
        harness.send(bar(MenuBarMsg::Title(1))).await;
        assert_eq!(open(&harness), None);

        harness.send(bar(MenuBarMsg::HoverTitle(1))).await;
        assert_eq!(open(&harness), None, "hovering opens nothing while closed");
        harness.send(bar(MenuBarMsg::Title(2))).await;
        harness.press("F10").await;
        assert_eq!(open(&harness), None);
        assert!(harness.app.quit.asking.is_none());
        harness.send(bar(MenuBarMsg::Title(2))).await;
        harness.press("Escape").await;
        assert_eq!(open(&harness), None);
    }

    /// The setting off hides the bar: Alt with a letter opens nothing.
    #[tokio::test]
    async fn the_setting_off_hides_the_bar() {
        let (_root, mut harness) = Harness::temp_with_db().await;
        harness.send(bar(MenuBarMsg::Shown(false))).await;
        harness.press("Alt+f").await;
        assert_eq!(harness.app.dialog_windows.menu_bar.open, None);
    }
}
