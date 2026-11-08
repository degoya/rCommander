use rc_engine::{
    BarButton, BarItem, ButtonAction, CommandKind, CommandRef, IconRef, Keymap,
    button_bars::{DEFAULT_ROWS, default_spec},
};

use super::{ROWS, collection_among, is_collection, of, of_row};
use crate::theme::icon::Icon;

fn rows(command: &str) -> BarButton {
    BarButton {
        action: ButtonAction::Command(CommandRef {
            kind: CommandKind::Internal,
            command: command.into(),
            ..CommandRef::default()
        }),
        icon: None,
        tooltip: String::new(),
    }
}

#[test]
fn every_button_of_the_default_bar_has_an_icon() {
    for row in DEFAULT_ROWS.iter().flatten() {
        assert!(of_row(row).is_some(), "{row}");
    }
    for item in default_spec().items {
        if let BarItem::Button(button) = item {
            assert!(of(&button).is_some(), "{:?}", button.action);
        }
    }
}

#[test]
fn every_row_with_a_glyph_is_a_row_of_the_standard_keymap() {
    let keymap = Keymap::standard();
    for (row, _) in ROWS {
        assert!(
            keymap.bindings().iter().any(|binding| binding.name == *row),
            "{row}"
        );
    }
}

#[test]
fn a_total_commander_name_a_symbol_file_and_a_program_decide_alike() {
    assert_eq!(of(&rows("cm_SrcFull")), Some(Icon::FullView));
    assert_eq!(of(&rows("view_brief,panel.refresh")), Some(Icon::BriefView));
    assert_eq!(
        of(&rows("panel.toggle_hidden")),
        None,
        "no glyph: the text shows"
    );
    let mut own_symbol = rows("panel.refresh");
    own_symbol.icon = Some(IconRef {
        path: "/icons/reread.png".into(),
        index: 0,
    });
    assert_eq!(of(&own_symbol), None, "its own symbol goes first");
    let program = BarButton {
        action: ButtonAction::Command(CommandRef {
            command: "notepad.exe".into(),
            ..CommandRef::default()
        }),
        icon: None,
        tooltip: String::new(),
    };
    assert_eq!(of(&program), None);
}

fn own_button(name: &str) -> BarButton {
    BarButton {
        action: ButtonAction::Command(CommandRef {
            kind: CommandKind::Own,
            command: name.into(),
            ..CommandRef::default()
        }),
        icon: None,
        tooltip: String::new(),
    }
}

fn own_command(icon_path: &str) -> rc_engine::UserCommand {
    rc_engine::UserCommand {
        id: 1,
        name: "em_np".into(),
        action: CommandRef::default(),
        tooltip: "Notepad".into(),
        icon_path: icon_path.into(),
        icon_index: 0,
    }
}

/// RC-110-10: a symbol of the collection shows for a button and for an own command's button; a
/// name the collection does not have, a file and Total Commander's `WCMICONS.DLL` give no glyph,
/// so the button shows its symbol file or its text as before.
#[test]
fn a_symbol_of_the_collection_shows_and_an_unknown_name_leaves_the_text() {
    let folder = rc_engine::user_commands::symbols::find("folder").expect("listed");
    let mut button = rows("panel.refresh");
    button.icon = Some(IconRef {
        path: "rc-symbol:folder".into(),
        index: 0,
    });
    assert_eq!(collection_among(&button, &[]), Some(folder.glyph));
    assert_eq!(
        of(&button),
        None,
        "its own symbol goes before the row's glyph"
    );
    button.icon = Some(IconRef {
        path: "rc-symbol:no_such_symbol".into(),
        index: 0,
    });
    assert_eq!(collection_among(&button, &[]), None, "the text shows");
    assert_eq!(of(&button), None);

    let own = own_button("EM_NP");
    assert_eq!(
        collection_among(&own, &[own_command(" rc-symbol:folder ")]),
        Some(folder.glyph)
    );
    assert_eq!(
        collection_among(&own, &[own_command("rc-symbol:no_such_symbol")]),
        None
    );
    assert_eq!(
        collection_among(&own, &[own_command("%COMMANDER_PATH%\\WCMICONS.DLL")]),
        None,
        "an imported file stays a file"
    );
    assert_eq!(collection_among(&own, &[]), None, "no such own command");
    assert!(is_collection("rc-symbol:no_such_symbol"));
    assert!(!is_collection("C:\\totalcmd\\WCMICONS.DLL"));
}
