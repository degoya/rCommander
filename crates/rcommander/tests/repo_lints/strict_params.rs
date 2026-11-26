//! Guards the rule that an MCP tool refuses an argument it does not take (RC-110-57): every type
//! under `crates/rc-mcp/src/tools/` that derives `Deserialize` and takes keys - a struct with
//! named fields, an internally tagged enum (`#[serde(tag = ...)]`) - carries
//! `#[serde(deny_unknown_fields)]`, and with it `additionalProperties: false` in its schema. A
//! type without it lets a typo in a key through, dropped in silence, as `ops_move
//! {"source": [...]}` once moved the panel's selection instead of the file it named.

use crate::{files_with, relative, scan::code_only, workspace_root};

/// The lines of the types in `source` that take keys without denying unknown ones.
fn open_types(source: &str) -> Vec<usize> {
    let code = code_only(source);
    let lines: Vec<&str> = code.lines().collect();
    let mut found = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        if !lines[at].trim_start().starts_with("#[derive(") {
            at += 1;
            continue;
        }
        let mut attributes = String::new();
        let mut item = at;
        while item < lines.len() && !is_item(lines[item]) {
            attributes.push_str(lines[item]);
            attributes.push(' ');
            item += 1;
        }
        let Some(header) = lines.get(item) else {
            break;
        };
        let derives = attributes.split("#[derive(").skip(1).any(|derive| {
            derive
                .split(")]")
                .next()
                .is_some_and(|list| list.contains("Deserialize"))
        });
        let named_struct = header.contains("struct ") && header.trim_end().ends_with('{');
        let tagged_enum = header.contains("enum ") && attributes.contains("tag =");
        if derives && (named_struct || tagged_enum) && !attributes.contains("deny_unknown_fields") {
            found.push(item + 1);
        }
        at = item + 1;
    }
    found
}

/// Whether `line` starts a struct or an enum.
fn is_item(line: &str) -> bool {
    line.split_whitespace()
        .take_while(|word| !word.starts_with("#["))
        .any(|word| word == "struct" || word == "enum")
}

#[test]
fn every_tool_parameter_type_denies_unknown_fields() {
    let root = workspace_root();
    let mut sources = Vec::new();
    files_with(
        &root.join("crates/rc-mcp/src/tools"),
        &[".rs"],
        &mut sources,
    );
    assert!(sources.len() > 50, "expected to scan rc-mcp's tools");
    let mut found = Vec::new();
    for path in &sources {
        let shown = relative(&root, path);
        if shown.ends_with("_tests.rs") {
            continue;
        }
        let source = std::fs::read_to_string(path).expect("read source");
        for line in open_types(&source) {
            found.push(format!("{shown}:{line}"));
        }
    }
    assert!(
        found.is_empty(),
        "a tool's parameter type without #[serde(deny_unknown_fields)] (RC-110-57):\n  {}",
        found.join("\n  ")
    );
}

#[test]
fn the_open_shapes_are_found_and_the_closed_ones_are_not() {
    let source = "#[derive(Deserialize, schemars::JsonSchema)]\n\
                  pub(crate) struct Open {\n\
                  pub a: String,\n\
                  }\n\
                  #[derive(Deserialize, schemars::JsonSchema)]\n\
                  #[serde(deny_unknown_fields)]\n\
                  /// A comment between.\n\
                  pub(crate) struct Closed {\n\
                  pub a: String,\n\
                  }\n\
                  #[derive(Deserialize)]\n\
                  #[serde(tag = \"kind\", rename_all = \"snake_case\")]\n\
                  enum OpenTagged {\n\
                  A { x: u8 },\n\
                  }\n\
                  #[derive(Deserialize)]\n\
                  #[serde(tag = \"kind\", deny_unknown_fields)]\n\
                  enum ClosedTagged {\n\
                  A { x: u8 },\n\
                  }\n\
                  #[derive(Debug, Clone, Copy, Deserialize)]\n\
                  #[serde(rename_all = \"snake_case\")]\n\
                  enum Plain {\n\
                  A,\n\
                  }\n\
                  #[derive(Serialize)]\n\
                  struct Answer {\n\
                  a: u8,\n\
                  }\n\
                  #[derive(Deserialize)]\n\
                  struct Wrapped(String);\n";
    assert_eq!(open_types(source), [2, 13]);
}
