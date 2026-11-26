use rmcp::model::{CallToolResponse, CallToolResult, ContentBlock};
use serde_json::{Map, Value, json};

use super::{as_invalid_params, check};
use crate::RcMcpServer;

/// The input schema of the release tool `tool`.
fn schema_of(tool: &str) -> Map<String, Value> {
    let router = RcMcpServer::router();
    let listed = router.get(tool).unwrap_or_else(|| panic!("no tool {tool}"));
    (*listed.input_schema).clone()
}

/// What `check` answers for `arguments` of `tool`: `None` when they fit, else the error body.
fn refusal(tool: &str, arguments: Value) -> Option<Value> {
    let arguments = arguments.as_object().cloned().expect("an object");
    check(tool, &schema_of(tool), Some(&arguments))
        .err()
        .map(|result| body(&result))
}

fn body(result: &CallToolResult) -> Value {
    assert_eq!(result.is_error, Some(true));
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    serde_json::from_str(&text).expect("json")
}

/// The message of a refusal with the code `invalid_params`.
fn message(refused: Option<Value>) -> String {
    let refused = refused.expect("refused");
    assert_eq!(refused["code"], "invalid_params", "{refused}");
    refused["error"].as_str().expect("message").to_owned()
}

#[test]
fn a_typo_in_a_key_is_refused_with_the_names_the_tool_takes() {
    // The audit's case: `source` for `sources` used to move the panel's selection.
    let error = message(refusal("ops_move", json!({ "source": ["a.txt"] })));
    assert!(
        error.starts_with("ops_move: unknown argument `source`"),
        "{error}"
    );
    assert!(error.contains("sources"), "{error}");
    assert!(error.contains("nothing was done"), "{error}");
    assert!(!error.contains("a.txt"), "never the value: {error}");
}

#[test]
fn the_right_keys_and_the_code_argument_fit() {
    assert_eq!(
        refusal(
            "ops_move",
            json!({ "sources": ["a.txt"], "target": "/tmp", "confirmation": "abc" })
        ),
        None
    );
    // Every tool takes the code, the gate's argument, even where its schema does not name it.
    assert_eq!(
        refusal("get_panel_state", json!({ "confirmation": "abc" })),
        None
    );
    assert_eq!(refusal("ops_move", json!({})), None);
}

#[test]
fn a_tool_without_arguments_takes_none() {
    let error = message(refusal("vault_status", json!({ "panel": "left" })));
    assert!(error.contains("unknown argument `panel`"), "{error}");
    assert!(error.contains("takes no arguments"), "{error}");
    assert_eq!(refusal("vault_status", json!({})), None);
}

#[test]
fn a_wrong_type_a_missing_argument_and_a_value_off_the_list_are_named() {
    let error = message(refusal("ops_move", json!({ "sources": "a.txt" })));
    assert!(error.contains("`sources` must be array"), "{error}");
    let error = message(refusal("navigate", json!({})));
    assert!(error.contains("missing argument `path`"), "{error}");
    let error = message(refusal("list_dir", json!({ "panel": "middle" })));
    assert!(error.contains("`panel` must be one of"), "{error}");
    assert!(error.contains("left") && error.contains("right"), "{error}");
}

#[test]
fn a_typo_deeper_down_is_named_with_its_path() {
    let error = message(refusal(
        "files_search",
        json!({ "content": { "text": "x", "regexx": true } }),
    ));
    assert!(
        error.contains("unknown argument `content.regexx`"),
        "{error}"
    );
    let error = message(refusal(
        "login_scripts_create",
        json!({ "name": "a", "steps": [{ "step": "send_text", "text": "x", "newlin": true }] }),
    ));
    assert!(error.contains("`steps[0].newlin`"), "{error}");
    assert_eq!(
        refusal(
            "login_scripts_create",
            json!({ "name": "a", "steps": [{ "step": "send_text", "text": "x", "newline": true }] }),
        ),
        None
    );
}

#[test]
fn a_limit_above_its_maximum_is_refused_not_cut() {
    for tool in [
        "files_search",
        "list_dir",
        "archives_list",
        "connections_list",
    ] {
        let error = message(refusal(tool, json!({ "limit": 1001 })));
        assert!(
            error.contains("`limit` must be at most 1000"),
            "{tool}: {error}"
        );
        let error = message(refusal(tool, json!({ "limit": 0 })));
        assert!(
            error.contains("`limit` must be at least 1"),
            "{tool}: {error}"
        );
        assert_eq!(refusal(tool, json!({ "limit": 1000 })), None, "{tool}");
    }
    let error = message(refusal("dirs_compare", json!({ "limit": 5001 })));
    assert!(error.contains("at most 5000"), "{error}");
}

#[test]
fn references_and_forms_are_followed() {
    let schema = json!({
        "type": "object",
        "properties": {
            "rule": { "anyOf": [{ "$ref": "#/$defs/Rule" }, { "type": "null" }] },
            "mode": { "oneOf": [{ "const": "fast" }, { "const": "slow" }] }
        },
        "additionalProperties": false,
        "$defs": {
            "Rule": {
                "type": "object",
                "properties": { "mask": { "type": "string" } },
                "required": ["mask"],
                "additionalProperties": false
            }
        }
    });
    let schema = schema.as_object().cloned().expect("object");
    let checked = |arguments: Value| {
        check("t", &schema, arguments.as_object())
            .err()
            .map(|result| body(&result)["error"].as_str().expect("message").to_owned())
    };
    assert_eq!(checked(json!({ "rule": { "mask": "*.rs" } })), None);
    assert_eq!(checked(json!({ "rule": null, "mode": "slow" })), None);
    let error = checked(json!({ "rule": { "mask": "*.rs", "masks": 1 } })).expect("refused");
    assert!(error.contains("unknown argument `rule.masks`"), "{error}");
    let error = checked(json!({ "rule": 5 })).expect("refused");
    assert!(error.contains("`rule` must be object or null"), "{error}");
    let error = checked(json!({ "mode": "medium" })).expect("refused");
    assert!(error.contains("one of fast, slow"), "{error}");
}

/// Every tool's schema says `additionalProperties: false` wherever it describes an object with
/// properties - what `deny_unknown_fields` on the parameter types writes; a tool without any
/// argument has an empty one, which [`check`] holds closed.
#[test]
fn every_tool_closes_its_schema() {
    let mut open = Vec::new();
    for tool in RcMcpServer::router().list_all() {
        let schema = Value::Object((*tool.input_schema).clone());
        let empty = schema["properties"].as_object().is_none_or(Map::is_empty);
        if !empty {
            closed(&tool.name, &schema, "", &mut open);
        }
    }
    assert!(
        open.is_empty(),
        "objects without additionalProperties: false: {open:?}"
    );
}

fn closed(tool: &str, schema: &Value, at: &str, open: &mut Vec<String>) {
    let Some(object) = schema.as_object() else {
        return;
    };
    if let Some(properties) = object.get("properties").and_then(Value::as_object) {
        if object.get("additionalProperties") != Some(&Value::Bool(false)) {
            open.push(format!("{tool}{at}"));
        }
        for (name, property) in properties {
            closed(tool, property, &format!("{at}.{name}"), open);
        }
    }
    for key in ["anyOf", "oneOf", "allOf", "prefixItems"] {
        for form in object
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            closed(tool, form, at, open);
        }
    }
    for key in ["items", "additionalProperties"] {
        if let Some(inner) = object.get(key) {
            closed(tool, inner, &format!("{at}[]"), open);
        }
    }
    for (name, definition) in object
        .get("$defs")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        closed(tool, definition, &format!("#{name}"), open);
    }
}

#[test]
fn the_tools_own_parse_failure_becomes_invalid_params() {
    let failed = CallToolResult::error(vec![ContentBlock::text(
        "failed to deserialize parameters: invalid type: integer, expected a string",
    )]);
    let Ok(CallToolResponse::Complete(mapped)) = as_invalid_params(Ok(failed.into())) else {
        panic!("a complete answer");
    };
    let mapped = body(&mapped);
    assert_eq!(mapped["code"], "invalid_params", "{mapped}");
    assert!(
        mapped["error"]
            .as_str()
            .is_some_and(|error| error.contains("expected a string")),
        "{mapped}"
    );

    let other = CallToolResult::error(vec![ContentBlock::text(r#"{"code":"not_found"}"#)]);
    let Ok(CallToolResponse::Complete(kept)) = as_invalid_params(Ok(other.into())) else {
        panic!("a complete answer");
    };
    assert_eq!(body(&kept)["code"], "not_found");

    let unknown_tool =
        as_invalid_params(Err(rmcp::ErrorData::invalid_params("tool not found", None)));
    assert!(
        unknown_tool.is_err(),
        "an unknown tool stays a protocol error"
    );
}
