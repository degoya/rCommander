use rc_engine::Drive;

use super::*;

/// An answer's field is read by its name, as the MCP sees it; a field it lacks or of another
/// shape is none.
#[test]
fn a_field_is_read_by_name() {
    let outcome = Outcome::Drives { drives: Vec::new() };
    assert_eq!(field::<Vec<Drive>>(&outcome, "drives"), Some(Vec::new()));
    assert_eq!(field::<Vec<Drive>>(&outcome, "workspaces"), None);
    assert_eq!(field::<i64>(&outcome, "drives"), None, "not a number");
}

/// The form's start switch reads `workspaces.on_start` from a `settings.get` answer: a number is
/// a workspace, empty or anything else none.
#[test]
fn the_start_workspace_is_read_from_the_setting() {
    let answer = |value: &str| Outcome::Settings {
        settings: vec![rc_engine::settings::SettingValue {
            key: ON_START_KEY.to_owned(),
            value: serde_json::Value::from(value),
            default: serde_json::Value::from(""),
            kind: "id".to_owned(),
            min: None,
            max: None,
            choices: Vec::new(),
            protected: false,
            restart: true,
        }],
    };
    assert_eq!(start_of(&answer("7")), Some(7));
    assert_eq!(start_of(&answer("")), None);
    assert_eq!(start_of(&answer("servers")), None);
    assert_eq!(start_of(&Outcome::Drives { drives: Vec::new() }), None);
}
