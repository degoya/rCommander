//! The generator of `args.rs` on schemas of the shapes `schemars` writes: placeholders per type,
//! the aims where their type fits, and an error for what it cannot fill.

use serde_json::{Map, Value, json};

use super::*;

fn targets() -> Targets {
    Targets {
        web: 7,
        script: 8,
        credential: 9,
        port: 2222,
        terminal: Some(3),
        workspace: Some(4),
    }
}

fn schema(value: Value) -> Map<String, Value> {
    value.as_object().cloned().expect("an object schema")
}

#[test]
fn every_required_property_gets_a_placeholder_of_its_type() {
    let input = schema(json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "when": { "type": "string", "format": "date-time" },
            "count": { "type": "integer", "format": "uint32", "minimum": 3 },
            "ratio": { "type": "number", "maximum": 0.5 },
            "force": { "type": "boolean" },
            "mode": { "type": "string", "enum": ["copy", "move"] },
            "kind": { "$ref": "#/$defs/Kind" },
            "maybe": { "type": ["string", "null"] },
            "masks": { "type": "array", "items": { "type": "string" } },
            "spec": true,
            "unasked": { "type": "string" },
            "confirmation": { "type": ["string", "null"], "default": null }
        },
        "required": ["name", "when", "count", "ratio", "force", "mode", "kind", "maybe", "masks", "spec"],
        "$defs": { "Kind": { "oneOf": [{ "const": "file" }, { "const": "folder" }] } }
    }));
    let made = arguments("sample", &input, &targets()).expect("generated");
    assert_eq!(
        made,
        json!({
            "name": "sweep",
            "when": "2026-01-01T00:00:00Z",
            "count": 3,
            "ratio": 0.5,
            "force": false,
            "mode": "copy",
            "kind": "file",
            "maybe": "sweep",
            "masks": ["sweep"],
            "spec": {}
        })
    );
}

#[test]
fn the_aimed_properties_get_their_targets_where_the_type_fits() {
    let input = schema(json!({
        "type": "object",
        "properties": {
            "id": { "type": "integer" },
            "panel": { "anyOf": [{ "$ref": "#/$defs/Panel" }, { "type": "null" }] },
            "paths": { "type": ["array", "null"], "items": { "type": "string" } },
            "terminal": { "type": "integer", "minimum": 0 },
            "port": { "type": ["integer", "null"] },
            "left": { "type": "boolean" }
        },
        "required": ["id"],
        "$defs": { "Panel": { "type": "string", "enum": ["active", "left", "right"] } }
    }));
    let made = arguments("login_scripts_get", &input, &targets()).expect("generated");
    assert_eq!(
        made,
        json!({
            "id": 8,
            "panel": "left",
            "paths": [REMOTE_FILE],
            "terminal": 3,
            "port": 2222,
            "left": false
        })
    );
    let by_connection = arguments("connections_get", &input, &targets()).expect("generated");
    assert_eq!(by_connection["id"], 7);
    let by_workspace = arguments("workspaces_apply", &input, &targets()).expect("generated");
    assert_eq!(by_workspace["id"], 4);
}

#[test]
fn a_tool_without_properties_takes_an_empty_object() {
    let input = schema(json!({ "type": "object" }));
    assert_eq!(
        arguments("app_about", &input, &targets()).expect("generated"),
        json!({})
    );
}

#[test]
fn a_required_property_the_generator_cannot_fill_is_an_error() {
    for (property, defs) in [
        (json!(false), json!({})),
        (json!({ "type": "tuple" }), json!({})),
        (json!({ "$ref": "#/$defs/Missing" }), json!({})),
        (
            json!({ "$ref": "#/$defs/Loop" }),
            json!({ "Loop": { "$ref": "#/$defs/Loop" } }),
        ),
        (json!({ "enum": [] }), json!({})),
        (json!({ "oneOf": [{ "type": "null" }] }), json!({})),
    ] {
        let input = schema(json!({
            "type": "object",
            "properties": { "odd": property },
            "required": ["odd"],
            "$defs": defs
        }));
        let error = arguments("new_tool", &input, &targets()).expect_err("cannot be filled");
        assert!(error.starts_with("new_tool: odd: "), "{error}");
    }
    let unlisted = schema(json!({ "type": "object", "required": ["ghost"] }));
    assert!(arguments("new_tool", &unlisted, &targets()).is_err());
}
