use rc_engine::{CommandRef, StartItem, StartItemKind, UserCommand};

use super::*;

fn item(id: i64, parent: Option<i64>, depth: usize, kind: StartItemKind) -> StartItem {
    StartItem {
        id,
        parent,
        depth,
        kind,
        title: format!("Row {id}"),
        action: None,
    }
}

/// Top: 1 Notepad, 2 Tools (3 Putty, 4 ---, 5 Deep (6 Regedit)), 7 Calc.
fn menu() -> Vec<StartItem> {
    vec![
        item(1, None, 0, StartItemKind::Item),
        item(2, None, 0, StartItemKind::Submenu),
        item(3, Some(2), 1, StartItemKind::Item),
        item(4, Some(2), 1, StartItemKind::Separator),
        item(5, Some(2), 1, StartItemKind::Submenu),
        item(6, Some(5), 2, StartItemKind::Item),
        item(7, None, 0, StartItemKind::Item),
    ]
}

fn command(id: i64, name: &str, tooltip: &str, program: &str) -> UserCommand {
    UserCommand {
        id,
        name: name.into(),
        action: CommandRef {
            command: program.into(),
            ..CommandRef::default()
        },
        tooltip: tooltip.into(),
        icon_path: String::new(),
        icon_index: 0,
    }
}

#[test]
fn the_filter_finds_name_tooltip_and_program_in_any_case() {
    let commands = [
        command(1, "em_np", "Notepad", "notepad.exe"),
        command(2, "em_putty", "", "putty.exe"),
        command(3, "em_calc", "Calculator", "calc.exe"),
    ];
    assert_eq!(shown_commands(&commands, ""), [1, 2, 3]);
    assert_eq!(shown_commands(&commands, "NOTE"), [1]);
    assert_eq!(shown_commands(&commands, "putty"), [2]);
    assert_eq!(shown_commands(&commands, "em_ exe"), [1, 2, 3]);
    assert_eq!(shown_commands(&commands, "calc pad"), Vec::<i64>::new());
}

#[test]
fn the_cursor_stays_on_its_row_or_takes_the_first() {
    assert_eq!(keep(&[1, 2, 3], Some(2)), Some(2));
    assert_eq!(keep(&[1, 3], Some(2)), Some(1));
    assert_eq!(keep(&[], Some(2)), None);
    assert_eq!(step(&[1, 2, 3], Some(2), 1), Some(3));
    assert_eq!(step(&[1, 2, 3], Some(3), 1), Some(3));
    assert_eq!(step(&[1, 2, 3], Some(2), isize::MIN), Some(1));
    assert_eq!(step(&[1, 2, 3], None, 1), Some(1));
}

#[test]
fn the_tree_keeps_menu_order_and_its_depths() {
    let menu = menu();
    assert_eq!(shown_items(&menu), [1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(siblings(&menu, None), [1, 2, 7]);
    assert_eq!(siblings(&menu, Some(2)), [3, 4, 5]);
    let depths: Vec<usize> = menu.iter().map(|item| item.depth).collect();
    assert_eq!(depths, [0, 0, 1, 1, 1, 2, 0]);
}

#[test]
fn a_row_moves_up_and_down_among_its_neighbours_only() {
    let menu = menu();
    let at = |parent, position| {
        Some(Place {
            parent,
            position: Some(position),
        })
    };
    assert_eq!(moved(&menu, 7, Move::Up), at(None, 1));
    assert_eq!(moved(&menu, 1, Move::Down), at(None, 1));
    assert_eq!(moved(&menu, 4, Move::Down), at(Some(2), 2));
    assert_eq!(moved(&menu, 1, Move::Up), None, "the first place");
    assert_eq!(moved(&menu, 5, Move::Down), None, "the last in its submenu");
    assert_eq!(moved(&menu, 99, Move::Up), None, "no such row");
}

#[test]
fn a_row_moves_into_the_submenu_above_and_out_after_its_own() {
    let menu = menu();
    assert_eq!(
        moved(&menu, 7, Move::Into),
        Some(Place {
            parent: Some(2),
            position: None,
        })
    );
    assert_eq!(moved(&menu, 2, Move::Into), None, "a plain entry above");
    assert_eq!(moved(&menu, 1, Move::Into), None, "nothing above");
    assert_eq!(
        moved(&menu, 6, Move::Out),
        Some(Place {
            parent: Some(2),
            position: Some(3),
        })
    );
    assert_eq!(
        moved(&menu, 3, Move::Out),
        Some(Place {
            parent: None,
            position: Some(2),
        })
    );
    assert_eq!(moved(&menu, 1, Move::Out), None, "the top has no outside");
}

#[test]
fn a_new_row_goes_right_after_the_cursor() {
    let menu = menu();
    assert_eq!(
        new_place(&menu, Some(3)),
        Place {
            parent: Some(2),
            position: Some(1),
        }
    );
    assert_eq!(
        new_place(&menu, None),
        Place {
            parent: None,
            position: None,
        }
    );
}
