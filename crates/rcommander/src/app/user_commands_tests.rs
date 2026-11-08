use rc_engine::{CommandRef, StartItem, StartItemKind, UserCommand};

use super::*;

fn command(id: i64, name: &str) -> UserCommand {
    UserCommand {
        id,
        name: name.into(),
        action: CommandRef {
            command: "notepad.exe".into(),
            ..CommandRef::default()
        },
        tooltip: String::new(),
        icon_path: String::new(),
        icon_index: 0,
    }
}

fn entry(id: i64) -> StartItem {
    StartItem {
        id,
        parent: None,
        depth: 0,
        kind: StartItemKind::Item,
        title: format!("Entry {id}"),
        action: Some(CommandRef::default()),
    }
}

#[test]
fn a_new_list_keeps_the_cursor_and_shows_its_form() {
    let mut window = CommandsWindow::default();
    window.refresh_commands(&[command(1, "em_a"), command(2, "em_b")]);
    assert_eq!(window.cursor, Some(1), "the first");
    window.select_command(Some(2), &[command(1, "em_a"), command(2, "em_b")]);
    window.refresh_commands(&[command(2, "em_b"), command(3, "em_c")]);
    assert_eq!(window.cursor, Some(2));
    assert_eq!(
        window.own.as_ref().map(|form| form.name.as_str()),
        Some("em_b")
    );
    window.refresh_commands(&[command(3, "em_c")]);
    assert_eq!(window.cursor, Some(3), "deleted elsewhere: the first");
    window.refresh_commands(&[]);
    assert_eq!(window.cursor, None);
    assert!(window.own.is_none());
}

#[test]
fn a_form_being_typed_survives_a_new_list() {
    let mut window = CommandsWindow::default();
    window.refresh_commands(&[command(1, "em_a")]);
    window.own = Some(OwnForm::blank());
    window.refresh_commands(&[command(1, "em_a"), command(2, "em_b")]);
    let form = window.own.as_ref().expect("the new form");
    assert_eq!(form.id, None, "still the new one");
    assert!(window.dirty());
    window.tab = Tab::Start;
    assert!(!window.dirty(), "the Start menu's form is another");
}

#[test]
fn the_start_menu_keeps_its_cursor_as_the_list_does() {
    let mut window = CommandsWindow::default();
    window.refresh_start(&[entry(4), entry(5)]);
    assert_eq!(window.start_cursor, Some(4));
    window.select_item(Some(5), &[entry(4), entry(5)]);
    window.refresh_start(&[entry(5)]);
    assert_eq!(window.start_cursor, Some(5));
    assert_eq!(
        window.start.as_ref().map(|form| form.title.as_str()),
        Some("Entry 5")
    );
}

#[test]
fn the_keyboard_starts_on_the_tabs_first_stop() {
    assert_eq!(first_stop(Tab::Own), FILTER_FIELD);
    assert_eq!(first_stop(Tab::Start), TREE_STOP_ID);
}
