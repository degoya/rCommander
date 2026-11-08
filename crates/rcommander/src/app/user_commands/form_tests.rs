use rc_engine::{CommandKind, CommandRef, Show, StartItem, StartItemKind, UserCommand};

use super::*;

fn stored() -> UserCommand {
    UserCommand {
        id: 4,
        name: "em_np".into(),
        action: CommandRef {
            command: "notepad.exe".into(),
            params: "\"%P%N\"".into(),
            ..CommandRef::default()
        },
        tooltip: "Notepad".into(),
        icon_path: "notepad.exe".into(),
        icon_index: 2,
    }
}

#[test]
fn a_stored_command_is_shown_unchanged_and_saved_with_its_symbol() {
    let mut form = OwnForm::of(&stored());
    assert!(!form.dirty);
    form.update(OwnMsg::Tooltip("Edit it".into()));
    form.update(OwnMsg::Name(" em_np ".into()));
    assert!(form.dirty);
    let draft = form.draft();
    assert_eq!(draft.name, "em_np", "trimmed");
    assert_eq!(draft.tooltip, "Edit it");
    assert_eq!(
        (draft.icon_path.as_str(), draft.icon_index),
        ("notepad.exe", 2)
    );
    assert_eq!(draft.action.params, "\"%P%N\"");
}

#[test]
fn a_new_command_starts_with_the_prefix_and_as_a_change() {
    let form = OwnForm::blank();
    assert_eq!(form.id, None);
    assert_eq!(form.name, "em_");
    assert!(form.dirty);
    assert_eq!(form.action.kind, CommandKind::Program);
}

#[test]
fn only_a_program_runs_as_administrator() {
    let mut action = CommandRef::default();
    edit(&mut action, CmdMsg::Elevated(true));
    assert!(action.elevated);
    edit(&mut action, CmdMsg::Kind(CommandKind::Internal));
    assert!(!action.elevated, "switching away drops it");
    edit(&mut action, CmdMsg::Elevated(true));
    assert!(!action.elevated, "rows never run elevated");
    edit(&mut action, CmdMsg::Show(Show::Maximized));
    edit(&mut action, CmdMsg::Folder("C:\\Temp".into()));
    assert_eq!(action.show, Show::Maximized);
    assert_eq!(action.folder, "C:\\Temp");
}

#[test]
fn a_start_row_saves_what_its_kind_has() {
    let item = StartItem {
        id: 9,
        parent: Some(2),
        depth: 1,
        kind: StartItemKind::Item,
        title: "&Notepad".into(),
        action: Some(CommandRef {
            command: "notepad.exe".into(),
            ..CommandRef::default()
        }),
    };
    let mut form = StartForm::of(&item);
    assert_eq!(form.place.parent, Some(2));
    let draft = form.draft();
    assert_eq!(draft.title, "&Notepad");
    assert_eq!(
        draft.action.map(|action| action.command),
        Some("notepad.exe".into())
    );
    form.update(StartMsg::Kind(StartItemKind::Submenu));
    let draft = form.draft();
    assert_eq!(draft.action, None, "a submenu has no command");
    assert_eq!(draft.title, "&Notepad");
    form.update(StartMsg::Kind(StartItemKind::Separator));
    assert_eq!(form.draft().title, "", "a separator has no title");
    form.update(StartMsg::Kind(StartItemKind::Item));
    assert_eq!(form.action.command, "notepad.exe", "the command was kept");
}

/// The symbol is edited in the form as on a button: its file and index, what is no number 0;
/// the preview reads the trimmed file at the bars' size, none without a file.
#[test]
fn the_symbol_is_edited_and_previewed() {
    let mut form = OwnForm::of(&stored());
    form.update(OwnMsg::IconPath(" C:\\Tools\\tools.dll ".into()));
    form.update(OwnMsg::IconIndex(" 7 ".into()));
    assert!(form.dirty);
    let key = form.symbol_key(24).expect("a symbol");
    assert_eq!(
        (key.path.as_str(), key.index, key.size),
        ("C:\\Tools\\tools.dll", 7, 24)
    );
    let draft = form.draft();
    assert_eq!(
        (draft.icon_path.as_str(), draft.icon_index),
        ("C:\\Tools\\tools.dll", 7)
    );
    form.update(OwnMsg::IconIndex("x".into()));
    assert_eq!(form.icon_index, 0);
    form.update(OwnMsg::IconPath("  ".into()));
    assert!(form.symbol_key(24).is_none(), "no file, no preview");
    assert_eq!(form.draft().icon_path, "");
}

/// RC-110-10: a symbol of the collection chosen replaces the file; it is a glyph, no file to read.
#[test]
fn a_symbol_of_the_collection_replaces_the_file() {
    let mut form = OwnForm::of(&stored());
    assert!(form.symbol_key(24).is_some(), "a file is read");
    form.choose_symbol("rc-symbol:folder".into());
    assert!(form.dirty);
    assert_eq!(form.symbol_key(24), None);
    let folder = rc_engine::user_commands::symbols::find("folder").expect("listed");
    assert_eq!(form.collection_glyph(), Some(folder.glyph));
    let draft = form.draft();
    assert_eq!(
        (draft.icon_path.as_str(), draft.icon_index),
        ("rc-symbol:folder", 0)
    );
    form.update(OwnMsg::IconPath("rc-symbol:no_such_symbol".into()));
    assert_eq!(form.collection_glyph(), None, "the preview stays empty");
    assert_eq!(form.symbol_key(24), None);
}
