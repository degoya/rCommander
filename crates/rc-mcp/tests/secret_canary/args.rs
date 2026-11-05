//! The arguments of each tool, read from its input schema (RC-080-61), not from a list of names:
//! every required property gets a valid placeholder of its type (a `$ref` followed, an enum's
//! first value, a list of one), and every property the sweep aims - the connection, the script,
//! the credential, the terminal, the workspace, a panel, the test server's file - gets its
//! target where its type takes it, required or not. A required property the generator cannot fill
//! is an error: the sweep fails on a new tool it could not drive into its core.

use serde_json::{Map, Value, json};

/// The file on the test server every path argument points at: whatever a tool reads, writes or
/// deletes there goes over SFTP with the stored password, never into the local system.
pub const REMOTE_FILE: &str = "sftp://web/notes.txt";

/// The argument a confirmation code travels in; the gate reads it, the sweep sets it itself.
const CODE: &str = "confirmation";

/// How deep a schema may nest before the generator gives up (a `$ref` that refers to itself).
const MAX_DEPTH: usize = 12;

/// The property names that hold a path, and so point at [`REMOTE_FILE`]. Not `key_path`: it
/// would change how the connection logs in for the rest of the sweep.
const PATH_NAMES: &[&str] = &[
    "path",
    "paths",
    "source",
    "sources",
    "target",
    "target_dir",
    "left",
    "right",
    "dir",
    "root",
    "archive",
    "archives",
    "link",
    "wincmd_ini",
    "ftp_ini",
];

/// What the sweep points the tools at.
#[derive(Clone, Debug)]
pub struct Targets {
    /// The connection with the stored password, on the test server.
    pub web: i64,
    /// The login script with the stored secret.
    pub script: i64,
    /// The shared credential with the stored password.
    pub credential: i64,
    /// The test server's port.
    pub port: u16,
    /// The terminal on the test server, once one is open.
    pub terminal: Option<u64>,
    /// The workspace that shows that terminal, once captured.
    pub workspace: Option<i64>,
}

impl Targets {
    /// The value the sweep gives property `name` of `tool`, if it aims that name.
    fn aimed(&self, tool: &str, name: &str) -> Option<Value> {
        let value = match name {
            "id" if tool.starts_with("login_scripts_") => json!(self.script),
            "id" if tool.starts_with("credentials_") => json!(self.credential),
            "id" if tool.starts_with("workspaces_") => json!(self.workspace.unwrap_or(self.web)),
            "id" | "connection_id" => json!(self.web),
            "script_id" | "login_script_id" => json!(self.script),
            "terminal" => json!(self.terminal.unwrap_or(0)),
            "panel" => json!("left"),
            "host" => json!("127.0.0.1"),
            "port" => json!(self.port),
            "username" => json!("tester"),
            "command" | "text" => json!("echo sweep"),
            _ if PATH_NAMES.contains(&name) => json!(REMOTE_FILE),
            _ => return None,
        };
        Some(value)
    }
}

/// The arguments of `tool` from its `input_schema`, or why none can be made.
pub fn arguments(
    tool: &str,
    input_schema: &Map<String, Value>,
    targets: &Targets,
) -> Result<Value, String> {
    let defs = input_schema
        .get("$defs")
        .or_else(|| input_schema.get("definitions"))
        .and_then(Value::as_object);
    let generator = Generator {
        tool,
        targets,
        defs,
    };
    generator
        .object(input_schema, 0)
        .map(Value::Object)
        .map_err(|why| format!("{tool}: {why}"))
}

struct Generator<'a> {
    tool: &'a str,
    targets: &'a Targets,
    defs: Option<&'a Map<String, Value>>,
}

impl Generator<'_> {
    /// An object schema's required properties, and the aimed ones it has.
    fn object(
        &self,
        schema: &Map<String, Value>,
        depth: usize,
    ) -> Result<Map<String, Value>, String> {
        let empty = Map::new();
        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        let required: Vec<&str> = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|names| names.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if let Some(missing) = required
            .iter()
            .find(|name| !properties.contains_key(**name))
        {
            return Err(format!("the required {missing} has no schema"));
        }
        let mut arguments = Map::new();
        for (name, property) in properties {
            if name == CODE {
                continue;
            }
            let aim = self.targets.aimed(self.tool, name);
            if required.contains(&name.as_str()) {
                let value = self
                    .value(property, aim.as_ref(), depth + 1)
                    .map_err(|why| format!("{name}: {why}"))?;
                arguments.insert(name.clone(), value);
            } else if let Some(aim) = aim
                && let Ok(value) = self.value(property, Some(&aim), depth + 1)
            {
                arguments.insert(name.clone(), value);
            }
        }
        Ok(arguments)
    }

    /// A value `schema` takes; `aim` where its type fits.
    fn value(&self, schema: &Value, aim: Option<&Value>, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err("nested too deep (a $ref to itself?)".to_owned());
        }
        match schema {
            Value::Bool(true) => Ok(aim.cloned().unwrap_or_else(|| json!({}))),
            Value::Bool(false) => Err("a schema no value fits".to_owned()),
            Value::Object(schema) => self.of_object(schema, aim, depth),
            other => Err(format!("not a schema: {other}")),
        }
    }

    fn of_object(
        &self,
        schema: &Map<String, Value>,
        aim: Option<&Value>,
        depth: usize,
    ) -> Result<Value, String> {
        if let Some(reference) = schema.get("$ref") {
            return self.value(self.resolve(reference)?, aim, depth + 1);
        }
        if let Some(constant) = schema.get("const") {
            return Ok(constant.clone());
        }
        if let Some(choices) = schema.get("enum") {
            let choices = choices.as_array().map(Vec::as_slice).unwrap_or_default();
            if let Some(aim) = aim.filter(|aim| choices.contains(aim)) {
                return Ok(aim.clone());
            }
            return choices
                .iter()
                .find(|choice| !choice.is_null())
                .or_else(|| choices.first())
                .cloned()
                .ok_or_else(|| "an empty enum".to_owned());
        }
        for key in ["oneOf", "anyOf", "allOf"] {
            if let Some(variants) = schema.get(key) {
                return self.variant(variants, aim, depth);
            }
        }
        let types: Vec<&str> = match schema.get("type") {
            Some(Value::String(one)) => vec![one.as_str()],
            Some(Value::Array(many)) => many.iter().filter_map(Value::as_str).collect(),
            Some(other) => return Err(format!("a type that is no name: {other}")),
            None if schema.contains_key("properties") => vec!["object"],
            None if schema.contains_key("items") => vec!["array"],
            None => return Ok(aim.cloned().unwrap_or_else(|| json!({}))),
        };
        let chosen = types
            .iter()
            .copied()
            .find(|kind| aim.is_some_and(|aim| fits(kind, aim)))
            .or_else(|| types.iter().copied().find(|kind| *kind != "null"))
            .or_else(|| types.first().copied())
            .ok_or_else(|| "no type".to_owned())?;
        self.of_type(chosen, schema, aim, depth)
    }

    /// The first variant a value can be made for, the one that takes `aim` before the others.
    fn variant(
        &self,
        variants: &Value,
        aim: Option<&Value>,
        depth: usize,
    ) -> Result<Value, String> {
        let variants = variants.as_array().map(Vec::as_slice).unwrap_or_default();
        let made: Vec<Value> = variants
            .iter()
            .filter(|variant| variant.get("type").and_then(Value::as_str) != Some("null"))
            .filter_map(|variant| self.value(variant, aim, depth + 1).ok())
            .collect();
        if let Some(aim) = aim
            && made.contains(aim)
        {
            return Ok(aim.clone());
        }
        made.into_iter()
            .next()
            .ok_or_else(|| "no variant a value can be made for".to_owned())
    }

    fn of_type(
        &self,
        kind: &str,
        schema: &Map<String, Value>,
        aim: Option<&Value>,
        depth: usize,
    ) -> Result<Value, String> {
        if let Some(aim) = aim.filter(|aim| fits(kind, aim)) {
            return Ok(aim.clone());
        }
        let bound = |key: &str| schema.get(key).and_then(Value::as_f64);
        Ok(match kind {
            "string" => json!(match schema.get("format").and_then(Value::as_str) {
                Some("date-time") => "2026-01-01T00:00:00Z",
                Some("date") => "2026-01-01",
                _ => "sweep",
            }),
            "integer" | "number" => {
                let mut number = 1.0_f64;
                if let Some(minimum) = bound("minimum") {
                    number = number.max(minimum);
                }
                if let Some(maximum) = bound("maximum") {
                    number = number.min(maximum);
                }
                if kind == "integer" {
                    json!(number.round() as i64)
                } else {
                    json!(number)
                }
            }
            "boolean" => json!(false),
            "null" => Value::Null,
            "array" => {
                let items = schema.get("items").unwrap_or(&Value::Bool(true));
                json!([self.value(items, aim, depth + 1)?])
            }
            "object" => Value::Object(self.object(schema, depth + 1)?),
            other => return Err(format!("a type the sweep cannot fill: {other}")),
        })
    }

    fn resolve(&self, reference: &Value) -> Result<&Value, String> {
        let name = reference
            .as_str()
            .and_then(|text| {
                text.strip_prefix("#/$defs/")
                    .or_else(|| text.strip_prefix("#/definitions/"))
            })
            .ok_or_else(|| format!("a $ref outside the schema: {reference}"))?;
        self.defs
            .and_then(|defs| defs.get(name))
            .ok_or_else(|| format!("a $ref to nothing: {reference}"))
    }
}

/// Whether a value of JSON type `kind` is `value`.
fn fits(kind: &str, value: &Value) -> bool {
    match kind {
        "string" => value.is_string(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "null" => value.is_null(),
        _ => false,
    }
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;
