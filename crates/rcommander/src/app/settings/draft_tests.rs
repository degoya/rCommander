use rc_engine::{
    CommandClass, RetentionLimits, RetentionStatus, SETTINGS, SettingValue,
    settings::{SHELL_KEY, THEME_KEY},
};
use rc_mcp::{Policy, Rule};
use serde_json::{Value, json};

use super::{Changes, Draft, Field, Invalid, Limit, Loaded, rule_of, rules_for};

fn value(key: &str, value: Value) -> SettingValue {
    SettingValue {
        key: key.to_owned(),
        value: value.clone(),
        default: value,
        kind: String::new(),
        min: None,
        max: None,
        choices: Vec::new(),
        protected: false,
        restart: false,
    }
}

fn loaded() -> Loaded {
    let values = SETTINGS
        .iter()
        .map(|spec| value(spec.key, spec.default_value()))
        .collect();
    let limits = |max_rows, max_age_days| RetentionLimits {
        max_rows,
        max_age_days,
    };
    Loaded {
        values,
        retention: vec![RetentionStatus {
            table: "error_log".into(),
            limits: limits(5_000, 30),
            defaults: limits(5_000, 30),
            floor: limits(100, 1),
            ceiling: limits(50_000, 365),
        }],
        policy: Policy::default(),
        shells: vec!["pwsh".into(), "cmd".into()],
        fonts: Vec::new(),
    }
}

#[test]
fn nothing_edited_is_nothing_to_store() {
    let draft = Draft::new(loaded());
    assert_eq!(draft.changes(), Ok(Changes::default()));
}

#[test]
fn every_schema_row_is_a_field() {
    let draft = Draft::new(loaded());
    for spec in SETTINGS {
        assert!(draft.field(spec.key).is_some(), "{}", spec.key);
    }
}

#[test]
fn edited_values_become_typed_changes() {
    let mut draft = Draft::new(loaded());
    draft.edit(THEME_KEY, Field::Text("dark".into()));
    draft.edit("mcp.http.enabled", Field::Bool(false));
    draft.edit("mcp.http.port", Field::Text(" 4711 ".into()));
    draft.edit_retention(0, Limit::Days, "7".into());
    draft.set_rule(CommandClass::Exec, Rule::Deny);
    let changes = draft.changes().expect("valid");
    assert_eq!(
        changes.settings,
        vec![
            (THEME_KEY, json!("dark")),
            ("mcp.http.enabled", json!(false)),
            ("mcp.http.port", json!(4711)),
        ]
    );
    assert_eq!(changes.retention, vec![("error_log".to_owned(), 5_000, 7)]);
    assert_eq!(changes.policy.map(|policy| policy.exec), Some(Rule::Deny));

    draft.stored(&changes);
    assert_eq!(
        draft.changes(),
        Ok(Changes::default()),
        "stored counts as loaded"
    );
}

#[test]
fn an_invalid_field_names_itself_and_nothing_is_stored() {
    let mut draft = Draft::new(loaded());
    draft.edit("mcp.http.port", Field::Text("70000".into()));
    assert_eq!(
        draft.changes(),
        Err(Invalid {
            label: "setting.mcp.http.port"
        })
    );

    let mut draft = Draft::new(loaded());
    draft.edit(SHELL_KEY, Field::Text("pwsh && calc".into()));
    assert!(draft.changes().is_err());

    let mut draft = Draft::new(loaded());
    draft.edit_retention(0, Limit::Rows, "0".into());
    assert_eq!(
        draft.changes(),
        Err(Invalid {
            label: "settings.retention.title"
        })
    );
}

#[test]
fn a_field_keeps_its_type() {
    let mut draft = Draft::new(loaded());
    draft.edit(THEME_KEY, Field::Bool(true));
    assert_eq!(draft.field(THEME_KEY), Some(&Field::Text("system".into())));
}

#[test]
fn destructive_and_exec_are_never_offered_without_a_confirmation() {
    for class in [CommandClass::Destructive, CommandClass::Exec] {
        assert!(!rules_for(class).contains(&Rule::Allowed));
    }
    let policy = Policy {
        destructive: Rule::Allowed,
        ..Policy::default()
    };
    assert_eq!(rule_of(&policy, CommandClass::Destructive), Rule::Ask);
}
