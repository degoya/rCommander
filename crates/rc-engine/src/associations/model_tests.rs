use serde_json::json;

use super::*;
use crate::{
    settings::{
        ASSOCIATIONS_ALL_MATCHES_KEY, ASSOCIATIONS_ENABLED_KEY, FILE_ASSOCIATIONS_KEY, SettingPage,
        is_protected, setting,
    },
    user_commands::{CommandKind, CommandRef},
};

fn program(command: &str, params: &str) -> CommandRef {
    CommandRef {
        command: command.into(),
        params: params.into(),
        ..CommandRef::default()
    }
}

fn rule(mask: &str, open: Option<CommandRef>) -> FileAssociation {
    FileAssociation {
        mask: mask.into(),
        open,
        ..FileAssociation::default()
    }
}

fn verb(name: &str, action: CommandRef) -> AssociationVerb {
    AssociationVerb {
        name: name.into(),
        action,
    }
}

fn kind_of(rules: &[FileAssociation]) -> RuleErrorKind {
    check_rules(rules).expect_err("refused").kind
}

#[test]
fn invalid_rules_are_refused() {
    let open = Some(program("notepad.exe", "\"%1\""));
    assert_eq!(kind_of(&[rule("  ", open.clone())]), RuleErrorKind::Mask);
    assert_eq!(
        kind_of(&[rule("*.txt\n", open.clone())]),
        RuleErrorKind::Text
    );
    let mut long = rule("*.txt", open.clone());
    long.description = "x".repeat(MAX_TEXT + 1);
    assert_eq!(kind_of(&[long]), RuleErrorKind::Text);
    let mut named_open = rule("*.txt", None);
    named_open.verbs = vec![verb("Open", program("a", "%1"))];
    assert_eq!(kind_of(&[named_open]), RuleErrorKind::VerbName);
    let mut twice = rule("*.txt", None);
    twice.verbs = vec![
        verb("view", program("a", "%1")),
        verb("VIEW", program("b", "%1")),
    ];
    let error = check_rules(&[twice]).expect_err("refused");
    assert_eq!(error.kind, RuleErrorKind::VerbTwice);
    assert_eq!((error.rule, error.verb.as_deref()), (Some(0), Some("VIEW")));
    let mut many = rule("*.txt", None);
    many.verbs = (0..=MAX_VERBS)
        .map(|at| verb(&format!("v{at}"), program("a", "%1")))
        .collect();
    assert_eq!(kind_of(&[many]), RuleErrorKind::TooManyVerbs);
    let elevated_rows = CommandRef {
        kind: CommandKind::Internal,
        command: "panel.refresh".into(),
        elevated: true,
        ..CommandRef::default()
    };
    assert!(matches!(
        kind_of(&[rule("*.txt", Some(elevated_rows))]),
        RuleErrorKind::Command(_)
    ));
    let too_many = vec![rule("*.txt", open.clone()); MAX_RULES + 1];
    assert_eq!(kind_of(&too_many), RuleErrorKind::TooMany);
    let big = vec![rule(&"*.x".repeat(300), open); MAX_RULES];
    assert_eq!(kind_of(&big), RuleErrorKind::TooLarge);
    assert_eq!(
        parse_rules("{\"mask\":\"*\"}").expect_err("no list").kind,
        RuleErrorKind::NotJson
    );
}

/// As in Total Commander, nothing is appended: a program without `%1`, `%2` or one of RC-100-86's
/// file placeholders is refused, and the refusal says which rule and command.
#[test]
fn a_program_without_file_parameter_is_refused() {
    let error = check_rules(&[rule("*.log", Some(program("notepad++.exe", "-multiInst")))])
        .expect_err("refused");
    assert_eq!(error.kind, RuleErrorKind::NeedsFileParameter);
    assert_eq!(error.kind.code(), "association_needs_file_parameter");
    assert_eq!((error.rule, error.verb.as_deref()), (Some(0), Some("open")));
    assert!(
        error
            .to_string()
            .starts_with("association_needs_file_parameter")
    );
    for params in [
        "%1", "\"%1\"", "%2", "%P%N", "-a %S", "@%L", "%UF", "?%WL", "\"%P%S\"",
    ] {
        let rules = [rule("*.log", Some(program("tool", params)))];
        assert!(check_rules(&rules).is_ok(), "{params}");
    }
    for params in ["", "%%1", "%P", "%O.%E", "%U", "%W1"] {
        assert!(!has_file_parameter(params), "{params}");
    }
    // Internal rows and own commands act on the panel; they need no file parameter.
    let rows = CommandRef {
        kind: CommandKind::Internal,
        command: "view_file".into(),
        ..CommandRef::default()
    };
    assert!(check_rules(&[rule("*.log", Some(rows))]).is_ok());
}

#[test]
fn rules_read_and_write_as_one_text() {
    assert_eq!(parse_rules("").expect("empty"), Vec::new());
    assert_eq!(parse_rules("  ").expect("blank"), Vec::new());
    assert_eq!(write_rules(&[]), "");
    let mut typed = rule(" *.log *.txt|read* ", Some(program(" notepad.exe ", "%1")));
    typed.verbs = vec![verb(" view ", program("less", "%N"))];
    typed.open = Some(CommandRef {
        command: String::new(),
        ..CommandRef::default()
    });
    let checked = check_rules(&[typed]).expect("fits");
    assert_eq!(checked[0].mask, "*.log *.txt|read*");
    assert_eq!(checked[0].open, None, "an open without a command is none");
    assert_eq!(checked[0].verbs[0].name, "view");
    let text = write_rules(&checked);
    assert_eq!(parse_rules(&text).expect("reads"), checked);
    assert_eq!(write_rules(&parse_rules(&text).expect("reads")), text);
}

#[test]
fn the_settings_are_protected_on_the_programs_page() {
    for key in [
        FILE_ASSOCIATIONS_KEY,
        ASSOCIATIONS_ENABLED_KEY,
        ASSOCIATIONS_ALL_MATCHES_KEY,
    ] {
        let spec = setting(key).expect("a row");
        assert!(is_protected(key), "{key}");
        assert_eq!(spec.page, SettingPage::Programs, "{key}");
    }
    let spec = setting(FILE_ASSOCIATIONS_KEY).expect("a row");
    assert_eq!(spec.type_name(), "file_associations");
    assert_eq!(spec.default_value(), json!(""));
    let rules = json!(write_rules(&[rule(
        "*.md",
        Some(program("gedit", "\"%P%N\""))
    )]));
    assert_eq!(spec.check(&rules), Ok(rules.clone()));
    let refused = spec
        .check(&json!(
            "[{\"mask\":\"*.md\",\"open\":{\"command\":\"gedit\"}}]"
        ))
        .expect_err("no file parameter");
    assert!(refused.starts_with("association_needs_file_parameter"));
    assert_eq!(
        setting(ASSOCIATIONS_ENABLED_KEY).map(crate::settings::SettingSpec::default_value),
        Some(json!(true))
    );
    assert_eq!(
        setting(ASSOCIATIONS_ALL_MATCHES_KEY).map(crate::settings::SettingSpec::default_value),
        Some(json!(false))
    );
}
