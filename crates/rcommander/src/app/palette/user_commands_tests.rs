use rc_engine::{Chord, CommandRef, Keymap, StartItem, StartItemKind, UserCommand};

use super::*;

fn command(id: i64, name: &str, tooltip: &str) -> UserCommand {
    UserCommand {
        id,
        name: name.into(),
        action: CommandRef {
            command: "notepad.exe".into(),
            ..CommandRef::default()
        },
        tooltip: tooltip.into(),
        icon_path: String::new(),
        icon_index: 0,
    }
}

fn row(id: i64, parent: Option<i64>, depth: usize, kind: StartItemKind, title: &str) -> StartItem {
    StartItem {
        id,
        parent,
        depth,
        kind,
        title: title.into(),
        action: (kind == StartItemKind::Item).then(CommandRef::default),
    }
}

/// Top: Notepad, ---, Tools (Putty, ---, Deep (Regedit)), ---, ---.
fn menu() -> Vec<StartItem> {
    vec![
        row(1, None, 0, StartItemKind::Item, "&Notepad"),
        row(2, None, 0, StartItemKind::Separator, ""),
        row(3, None, 0, StartItemKind::Submenu, "&Tools"),
        row(4, Some(3), 1, StartItemKind::Item, "Putty"),
        row(5, Some(3), 1, StartItemKind::Separator, ""),
        row(6, Some(3), 1, StartItemKind::Submenu, "Deep"),
        row(7, Some(6), 2, StartItemKind::Item, "Regedit"),
        row(8, None, 0, StartItemKind::Separator, ""),
        row(9, None, 0, StartItemKind::Separator, ""),
    ]
}

#[test]
fn a_title_loses_its_access_key_mark_only() {
    assert_eq!(plain_title("&Notepad"), "Notepad");
    assert_eq!(plain_title("Tom && Jerry"), "Tom & Jerry");
    assert_eq!(plain_title("Plain"), "Plain");
    assert_eq!(plain_title("end&"), "end");
}

#[test]
fn every_command_and_entry_is_a_row_that_runs_it() {
    let mut entries = Vec::new();
    let commands = [command(1, "em_np", "Notepad"), command(2, "em_putty", "  ")];
    add_rows(&mut entries, &commands, &menu());
    let mut labels: Vec<String> = entries.iter().map(|entry| entry.label.clone()).collect();
    labels.sort();
    let mut wanted = vec![
        command_label("em_putty"),
        command_label("Notepad"),
        start_label("Notepad"),
        start_label("Putty"),
        start_label("Regedit"),
    ];
    wanted.sort();
    assert_eq!(
        labels, wanted,
        "a submenu and a separator run nothing; a blank tooltip names the command"
    );
    let lower: Vec<String> = entries
        .iter()
        .map(|entry| entry.label.to_lowercase())
        .collect();
    assert!(lower.is_sorted(), "sorted by label");
    let notepad = entries
        .iter()
        .find(|entry| entry.name == "em_np")
        .expect("the command's row");
    assert_eq!(
        notepad.action,
        Action::Run(Box::new(Command::RunUserCommand {
            name: "em_np".into(),
            panel: None,
            params: None,
        }))
    );
    let regedit = entries
        .iter()
        .find(|entry| entry.name == "start:7")
        .expect("the entry's row");
    assert_eq!(
        regedit.action,
        Action::Run(Box::new(Command::RunStartItem {
            id: 7,
            panel: None,
            params: None,
        }))
    );
}

#[test]
fn a_command_with_a_key_names_the_keys_row() {
    let chord: Chord = "Ctrl+Alt+N".parse().expect("a chord");
    let mut entries = vec![Entry {
        name: "em_np".into(),
        label: "em_np".into(),
        keys: "Ctrl+Alt+N".into(),
        chords: "Ctrl+Alt+N".into(),
        action: Action::Press(chord),
        current: false,
    }];
    add_rows(&mut entries, &[command(1, "em_np", "Notepad")], &[]);
    assert_eq!(entries.len(), 1, "no second row");
    assert_eq!(entries[0].label, command_label("Notepad"));
    assert!(matches!(entries[0].action, Action::Press(_)));
}

#[test]
fn a_keys_name_reads_as_its_command_or_entry() {
    let commands = [command(1, "em_np", "Notepad")];
    let items = menu();
    assert_eq!(
        label_in("em_np", &commands, &items),
        Some(command_label("Notepad"))
    );
    assert_eq!(
        label_in("em_gone", &commands, &items),
        Some(command_label("em_gone"))
    );
    assert_eq!(
        label_in("start:4", &commands, &items),
        Some(start_label("Putty"))
    );
    assert_eq!(
        label_in("start:99", &commands, &items),
        Some(start_label("99"))
    );
    assert_eq!(label_in("copy", &commands, &items), None);
    assert!(missing("em_gone", &commands, &items));
    assert!(!missing("em_np", &commands, &items));
    assert!(
        missing("start:3", &commands, &items),
        "a submenu runs nothing"
    );
    assert!(!missing("start:7", &commands, &items));
    assert!(!missing("copy", &commands, &items));
}

#[test]
fn the_menu_nests_submenus_without_stray_separators() {
    let built = menu_items_of(&menu(), &Keymap::standard());
    assert_eq!(
        built,
        [
            BarItem::Entry {
                id: "start:1",
                text: "&Notepad".into(),
            },
            BarItem::Separator,
            BarItem::Submenu {
                text: "&Tools".into(),
                items: vec![
                    BarItem::Entry {
                        id: "start:4",
                        text: "Putty".into(),
                    },
                    BarItem::Separator,
                    BarItem::Submenu {
                        text: "Deep".into(),
                        items: vec![BarItem::Entry {
                            id: "start:7",
                            text: "Regedit".into(),
                        }],
                    },
                ],
            },
        ]
    );
    assert!(menu_items_of(&[], &Keymap::standard()).is_empty());
    assert!(menu_items("file", &Keymap::standard()).is_empty());
}

#[test]
fn an_entry_shows_its_key_after_a_tab() {
    let mut keymap = Keymap::standard();
    let chord: Chord = "Ctrl+Alt+F12".parse().expect("a chord");
    keymap
        .assign(chord, "start:1")
        .expect("a Start menu key binds");
    let built = menu_items_of(&menu()[..1], &keymap);
    let BarItem::Entry { text, .. } = &built[0] else {
        panic!("an entry: {built:?}");
    };
    assert!(text.starts_with("&Notepad\t"), "{text}");
}

#[test]
fn a_menu_choice_runs_its_entry() {
    assert_eq!(
        chosen("start:7"),
        Some(Command::RunStartItem {
            id: 7,
            panel: None,
            params: None,
        })
    );
    assert_eq!(chosen("copy"), None);
    assert_eq!(chosen("workspace:7"), None);
}
