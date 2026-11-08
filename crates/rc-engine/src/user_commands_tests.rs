use super::*;

#[test]
fn an_own_name_is_em_and_lower_case() {
    assert_eq!(clean_name(" EM_Putty ").expect("fits"), "em_putty");
    assert_eq!(clean_name("em_a_1").expect("fits"), "em_a_1");
    for name in [
        "putty",
        "em_",
        "em_put ty",
        "em_put-ty",
        "cm_copy",
        "em_put.ty",
    ] {
        assert_eq!(
            clean_name(name).expect_err("refused").code(),
            "user_command_name",
            "{name}"
        );
    }
    assert_eq!(own_name("em_x"), Some("x"));
    assert_eq!(own_name("em_X"), None, "kept names are lower case");
}

#[test]
fn row_names_are_keymap_tc_or_own_names() {
    for row in [
        "copy",
        "view_file",
        "panel.refresh",
        "cm_Copy",
        "CM_LIST",
        "em_putty",
    ] {
        assert!(is_row_name(row), "{row}");
    }
    for row in ["", "Copy", "cm_", "em_", "copy file", "cm_a-b", "../x"] {
        assert!(!is_row_name(row), "{row}");
    }
}

#[test]
fn a_question_mark_in_front_asks() {
    let action = CommandRef {
        params: " ?%P%N".into(),
        ..CommandRef::default()
    };
    assert!(action.asks());
    assert_eq!(action.params_to_ask(), "%P%N");
    let rows = CommandRef {
        kind: CommandKind::Internal,
        command: "copy".into(),
        params: "?x".into(),
        ..CommandRef::default()
    };
    assert!(!rows.asks(), "only a program asks");
}

#[test]
fn the_database_words_read_back() {
    for kind in [
        CommandKind::Program,
        CommandKind::Internal,
        CommandKind::Own,
    ] {
        assert_eq!(CommandKind::parse(kind.as_str()), Some(kind));
    }
    for show in [Show::Normal, Show::Minimized, Show::Maximized] {
        assert_eq!(Show::parse(show.as_str()), Some(show));
    }
    for kind in [
        StartItemKind::Item,
        StartItemKind::Submenu,
        StartItemKind::Separator,
    ] {
        assert_eq!(StartItemKind::parse(kind.as_str()), Some(kind));
    }
    assert_eq!(CommandKind::parse("script"), None);
    assert_eq!(Show::parse("hidden"), None);
}

#[test]
fn a_command_reads_from_json_with_defaults() {
    let draft: UserCommandDraft = serde_json::from_value(serde_json::json!({
        "name": "em_np",
        "action": { "command": "notepad.exe", "params": "\"%P%N\"" }
    }))
    .expect("reads");
    assert_eq!(draft.action.kind, CommandKind::Program);
    assert_eq!(draft.action.show, Show::Normal);
    assert!(!draft.action.elevated);
    let outcome = serde_json::to_value(UserCommandsOutcome::Ran {
        started: 1,
        asking: false,
    })
    .expect("writes");
    assert_eq!(outcome["answer"], "ran");
}

#[test]
fn every_code_is_listed_once() {
    let mut codes = UserCommandError::ALL_CODES.to_vec();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), UserCommandError::ALL_CODES.len());
    assert!(UserCommandError::ALL_CODES.contains(&UserCommandError::UnsafeArgument.code()));
    assert!(UserCommandError::ALL_CODES.contains(&UserCommandError::RunAsAdminUnsupported.code()));
}
