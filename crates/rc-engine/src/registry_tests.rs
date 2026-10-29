use super::{samples::one_of_each, *};
use crate::PanelId;

#[test]
fn every_command_has_a_row_and_every_row_a_command() {
    let mut from_commands: Vec<&str> = one_of_each().iter().map(Command::id).collect();
    from_commands.sort_unstable();
    let mut from_table: Vec<&str> = COMMANDS.iter().map(|spec| spec.id).collect();
    from_table.sort_unstable();
    assert_eq!(from_commands, from_table);
}

#[test]
fn each_group_is_sorted_and_ids_are_unique() {
    for group in GROUPS {
        let ids: Vec<&str> = group.iter().map(|spec| spec.id).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted, "keep each group's rows sorted by id");
    }
    let assembled: Vec<&str> = GROUPS
        .iter()
        .flat_map(|group| group.iter().map(|spec| spec.id))
        .collect();
    let ids: Vec<&str> = COMMANDS.iter().map(|spec| spec.id).collect();
    assert_eq!(ids, assembled);
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), ids.len(), "an id is in two rows");
}

#[test]
fn the_serde_tag_is_the_id() {
    for command in one_of_each() {
        let value = serde_json::to_value(&command).expect("serializes");
        assert_eq!(value["command"], command.id(), "{value}");
        let back: Command = serde_json::from_value(value).expect("deserializes");
        assert_eq!(back, command);
        assert_eq!(command.spec().id, command.id());
    }
}

#[test]
fn a_keymap_entry_reads_with_defaults() {
    let command: Command =
        serde_json::from_str(r#"{"command":"panel.navigate","params":{"path":".."}}"#)
            .expect("deserializes");
    assert_eq!(
        command,
        Command::Navigate {
            panel: None,
            path: "..".into()
        }
    );
    let cursor: Command = serde_json::from_str(
        r#"{"command":"panel.set_cursor","params":{"panel":"right","target":"notes.txt"}}"#,
    )
    .expect("deserializes");
    assert_eq!(
        cursor,
        Command::SetCursor {
            panel: Some(PanelId::Right),
            target: CursorTarget::Name("notes.txt".into())
        }
    );
}

#[test]
fn class_names_are_the_settings_spelling() {
    for class in CommandClass::ALL {
        assert_eq!(
            serde_json::to_value(class).expect("serializes"),
            class.as_str()
        );
    }
}

/// `Command` stays one flat, adjacently tagged enum - the groups' lists only write it - so a
/// wrong command says what is wrong: the unknown id, the missing or mistyped field, also inside
/// a request struct, and the keymap's error passes that on with the keys of its row.
#[test]
fn a_wrong_command_names_its_id_or_field() {
    let refused = |value: serde_json::Value| {
        serde_json::from_value::<Command>(value)
            .expect_err("refused")
            .to_string()
    };
    let unknown = refused(serde_json::json!({ "command": "panel.navigat", "params": {} }));
    assert!(
        unknown.contains("unknown variant `panel.navigat`"),
        "{unknown}"
    );
    let missing = refused(serde_json::json!({ "command": "panel.navigate", "params": {} }));
    assert!(missing.contains("missing field `path`"), "{missing}");
    let mistyped = refused(serde_json::json!({
        "command": "panel.navigate",
        "params": { "path": 5 }
    }));
    assert!(
        mistyped.contains("invalid type: integer `5`, expected a string"),
        "{mistyped}"
    );
    let in_request = refused(serde_json::json!({
        "command": "ops.copy",
        "params": { "sources": "a.txt" }
    }));
    assert!(
        in_request.contains("invalid type: string \"a.txt\", expected a sequence"),
        "{in_request}"
    );
    let keymap = crate::Keymap::parse(r#"[{ "keys": "F2", "command": "panel.no_such_command" }]"#)
        .expect_err("refused")
        .to_string();
    assert!(
        keymap.contains("F2") && keymap.contains("unknown variant `panel.no_such_command`"),
        "{keymap}"
    );
}
