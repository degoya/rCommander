//! The tool list as a contract (RC-110-60): every tool's name, class and input schema, held
//! against the snapshot `tests/tools.json`. A tool renamed, removed or added, an argument
//! renamed, retyped, made required or dropped, and a class changed fail here; descriptions do
//! not (the schema's `description` and `title` texts are left out), so wording stays free.
//!
//! A change on purpose rewrites the snapshot and says so in `CHANGELOG.md`:
//!
//! ```text
//! RC_MCP_WRITE_TOOLS=1 scripts/cargo.sh nextest run -p rc-mcp --test tool_contract
//! ```
//!
//! then `git diff crates/rc-mcp/tests/tools.json` shows what an agent will see differently.

use std::path::PathBuf;

use rc_mcp::{RcMcpServer, tool_classes};
use serde_json::{Map, Value, json};

/// The variable that rewrites the snapshot instead of comparing.
const WRITE: &str = "RC_MCP_WRITE_TOOLS";

/// The keys under which a schema names other schemas by name, not by keyword.
const NAMED: &[&str] = &["properties", "$defs", "definitions", "patternProperties"];

fn snapshot_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/tools.json")
}

/// `schema` without its prose: every `description` and `title` keyword, at every depth, but a
/// property that is named so.
fn without_prose(schema: &mut Value) {
    match schema {
        Value::Object(map) => {
            map.remove("description");
            map.remove("title");
            for (key, child) in map.iter_mut() {
                match child {
                    Value::Object(named) if NAMED.contains(&key.as_str()) => {
                        named.values_mut().for_each(without_prose);
                    }
                    _ => without_prose(child),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(without_prose),
        _ => {}
    }
}

/// Every tool as the snapshot holds it, by name.
fn contract() -> Value {
    let classes = tool_classes();
    let mut tools = Map::new();
    for tool in RcMcpServer::tools() {
        let mut schema = Value::Object((*tool.input_schema).clone());
        without_prose(&mut schema);
        let class = classes
            .get(tool.name.as_ref())
            .map_or("none", |class| class.as_str());
        tools.insert(
            tool.name.to_string(),
            json!({ "class": class, "input_schema": schema }),
        );
    }
    Value::Object(tools)
}

#[test]
fn the_tool_list_matches_its_snapshot() {
    let current = contract();
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(&current).expect("serializes")
    );
    let path = snapshot_path();
    if std::env::var_os(WRITE).is_some() {
        std::fs::write(&path, text).expect("the snapshot is written");
        return;
    }
    let Ok(stored) = std::fs::read_to_string(&path) else {
        panic!(
            "{} is missing; write it with {WRITE}=1 scripts/cargo.sh nextest run -p rc-mcp \
             --test tool_contract and commit it",
            path.display()
        );
    };
    let stored: Value = serde_json::from_str(&stored).expect("the snapshot is JSON");
    if stored == current {
        return;
    }
    let (Value::Object(stored), Value::Object(current)) = (&stored, &current) else {
        panic!("the snapshot is not an object of tools");
    };
    let mut changed: Vec<String> = Vec::new();
    for (name, tool) in current {
        match stored.get(name) {
            None => changed.push(format!("added: {name}")),
            Some(held) if held != tool => changed.push(format!("changed: {name}")),
            Some(_) => {}
        }
    }
    for name in stored.keys().filter(|name| !current.contains_key(*name)) {
        changed.push(format!("removed: {name}"));
    }
    panic!(
        "the MCP tool contract changed ({}). A change on purpose: rewrite the snapshot with \
         {WRITE}=1 scripts/cargo.sh nextest run -p rc-mcp --test tool_contract, and name it in \
         CHANGELOG.md",
        changed.join(", ")
    );
}

#[test]
fn prose_is_left_out_but_a_property_named_description_stays() {
    let mut schema = json!({
        "description": "x",
        "title": "Params",
        "type": "object",
        "properties": {
            "description": { "type": "string", "description": "the notes" },
            "title": { "type": "string", "title": "Title" },
            "mode": { "oneOf": [{ "const": "a", "description": "first" }] }
        }
    });
    without_prose(&mut schema);
    assert_eq!(
        schema,
        json!({
            "type": "object",
            "properties": {
                "description": { "type": "string" },
                "title": { "type": "string" },
                "mode": { "oneOf": [{ "const": "a" }] }
            }
        })
    );
}
