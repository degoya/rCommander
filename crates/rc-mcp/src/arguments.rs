//! A call's arguments held against its tool's input schema before the gate (RC-110-57).
//!
//! Every parameter type of a tool denies unknown fields, so its schema says
//! `additionalProperties: false`; a key the tool does not take - a typo like `source` for
//! `sources`, which used to be dropped and left the call to the panel's selection - is refused
//! here, with the other faults the schema can tell (a missing argument, a wrong type, a value
//! not in its list, a `limit` above its maximum). The refusal is the tool error
//! `{"code": "invalid_params"}` naming the argument, never its value, and comes before the gate:
//! nobody is asked to allow a call that would not parse. The code argument ([`CODE_ARGUMENT`])
//! belongs to the gate and is taken by every tool.
//!
//! The check reads the subset of JSON Schema that `schemars` writes for the parameter types and
//! is never stricter than their `Deserialize`: what it does not know it lets through, and the
//! tool's own parse still answers a fault the schema cannot express ([`as_invalid_params`]).

use rmcp::model::{CallToolResponse, CallToolResult};
use serde_json::{Map, Value};

use crate::{answer::error_result, confirm::CODE_ARGUMENT};

/// The longest argument name a refusal repeats.
const NAME_MAX: usize = 64;

/// Nesting deeper than this, `$ref`s included, is left to the tool's own parse.
const DEPTH_MAX: usize = 32;

/// What rmcp's `Parameters` answers when the tool's own parse fails.
const PARSE_FAILED: &str = "failed to deserialize parameters";

/// What is wrong with one argument.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Fault {
    /// A key the object does not take; the names it does take.
    Unknown(Vec<String>),
    Missing,
    /// Not of the type(s) named.
    Type(String),
    /// Not one of the values listed.
    NotListed(Vec<String>),
    TooSmall(String),
    TooLarge(String),
    TooShort,
    TooLong,
    /// None of the forms an `anyOf` or `oneOf` offers fits.
    NoForm,
}

impl Fault {
    /// Whether the value was of the right shape and failed on what it holds: of two forms that
    /// do not fit, the one whose fault is such is the form the caller meant.
    fn within(&self) -> bool {
        !matches!(self, Self::Type(_) | Self::NotListed(_) | Self::NoForm)
    }
}

/// A fault and where: `sources`, `content.text`, `rules[2].by`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Misfit {
    at: String,
    fault: Fault,
}

impl Misfit {
    fn describe(&self) -> String {
        let at = &self.at;
        match &self.fault {
            Fault::Unknown(known) if known.is_empty() => {
                format!("unknown argument `{at}`; it takes no arguments here")
            }
            Fault::Unknown(known) => {
                format!("unknown argument `{at}`; it takes {}", known.join(", "))
            }
            Fault::Missing => format!("missing argument `{at}`"),
            Fault::Type(expected) => format!("argument `{at}` must be {expected}"),
            Fault::NotListed(values) => {
                format!("argument `{at}` must be one of {}", values.join(", "))
            }
            Fault::TooSmall(bound) => format!("argument `{at}` must be at least {bound}"),
            Fault::TooLarge(bound) => format!("argument `{at}` must be at most {bound}"),
            Fault::TooShort => format!("argument `{at}` is too short"),
            Fault::TooLong => format!("argument `{at}` is too long"),
            Fault::NoForm => format!("argument `{at}` matches none of its forms"),
        }
    }
}

/// `arguments` of `tool` against its input `schema`; the refusal says which argument is at
/// fault. A schema without any property (a tool that takes no arguments) takes none.
///
/// Public only as the fuzz target's way in (`fuzz/`, RC-110-68, re-exported as
/// `check_arguments`); not an API.
#[doc(hidden)]
pub fn check(
    tool: &str,
    schema: &Map<String, Value>,
    arguments: Option<&Map<String, Value>>,
) -> Result<(), CallToolResult> {
    let mut arguments = arguments.cloned().unwrap_or_default();
    arguments.remove(CODE_ARGUMENT);
    let mut root = schema.clone();
    let takes_none = root
        .get("properties")
        .and_then(Value::as_object)
        .is_none_or(Map::is_empty);
    if takes_none && !root.contains_key("additionalProperties") {
        root.insert("additionalProperties".to_owned(), Value::Bool(false));
    }
    let checker = Checker { root: schema };
    let schema = Value::Object(root);
    checker
        .fits(&schema, &Value::Object(arguments), "", 0)
        .map_err(|misfit| {
            error_result(
                "invalid_params",
                &format!("{tool}: {}; nothing was done", misfit.describe()),
            )
        })
}

/// The tool's own parse failing - what rmcp answers as plain text - as the same
/// `invalid_params` error [`check`] answers; every other answer as it is.
pub(crate) fn as_invalid_params(
    answer: Result<CallToolResponse, rmcp::ErrorData>,
) -> Result<CallToolResponse, rmcp::ErrorData> {
    let failed = |text: &str| text.starts_with(PARSE_FAILED);
    match answer {
        Ok(CallToolResponse::Complete(result)) if result.is_error == Some(true) => {
            let text = result
                .content
                .first()
                .and_then(|content| content.as_text())
                .map(|text| text.text.clone());
            match text {
                Some(text) if failed(&text) => Ok(error_result("invalid_params", &text).into()),
                _ => Ok(result.into()),
            }
        }
        Err(error) if failed(&error.message) => {
            Ok(error_result("invalid_params", &error.message).into())
        }
        other => other,
    }
}

struct Checker<'a> {
    /// The tool's whole schema, where `$ref`s point.
    root: &'a Map<String, Value>,
}

impl Checker<'_> {
    fn fits(&self, schema: &Value, value: &Value, at: &str, depth: usize) -> Result<(), Misfit> {
        let schema = match schema {
            Value::Object(schema) if depth <= DEPTH_MAX => schema,
            Value::Bool(false) => return Err(misfit(at, Fault::NoForm)),
            _ => return Ok(()),
        };
        if let Some(target) = schema
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|reference| self.resolve(reference))
        {
            self.fits(target, value, at, depth + 1)?;
        }
        if let Some(all) = schema.get("allOf").and_then(Value::as_array) {
            for form in all {
                self.fits(form, value, at, depth + 1)?;
            }
        }
        for key in ["anyOf", "oneOf"] {
            if let Some(forms) = schema.get(key).and_then(Value::as_array) {
                self.one_of(forms, value, at, depth)?;
            }
        }
        typed(schema, value, at)?;
        listed(schema, value, at)?;
        bounded(schema, value, at)?;
        match value {
            Value::Array(items) => self.items(schema, items, at, depth),
            Value::Object(object) => self.object(schema, object, at, depth),
            _ => Ok(()),
        }
    }

    /// The first form `value` fits. None: the fault of the form the value came closest to -
    /// one it failed inside of - or, where every form failed on the value itself, the types or
    /// values they take together.
    fn one_of(&self, forms: &[Value], value: &Value, at: &str, depth: usize) -> Result<(), Misfit> {
        if forms.is_empty() {
            return Ok(());
        }
        let mut misfits = Vec::new();
        for form in forms {
            match self.fits(form, value, at, depth + 1) {
                Ok(()) => return Ok(()),
                Err(misfit) => misfits.push(misfit),
            }
        }
        let inside = misfits
            .iter()
            .filter(|misfit| misfit.fault.within() || misfit.at != at)
            .max_by_key(|misfit| (misfit.fault.within(), misfit.at.len()));
        if let Some(inside) = inside {
            return Err(inside.clone());
        }
        let mut values = Vec::new();
        let mut kinds = Vec::new();
        for misfit in misfits {
            match misfit.fault {
                Fault::NotListed(listed) => values.extend(listed),
                Fault::Type(kind) => kinds.push(kind),
                _ => {}
            }
        }
        let fault = if !values.is_empty() {
            Fault::NotListed(values)
        } else if !kinds.is_empty() {
            Fault::Type(kinds.join(" or "))
        } else {
            Fault::NoForm
        };
        Err(misfit(at, fault))
    }

    fn items(
        &self,
        schema: &Map<String, Value>,
        items: &[Value],
        at: &str,
        depth: usize,
    ) -> Result<(), Misfit> {
        let prefix = schema
            .get("prefixItems")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        for (index, item) in items.iter().enumerate() {
            if let Some(form) = prefix.get(index).or_else(|| schema.get("items")) {
                self.fits(form, item, &format!("{at}[{index}]"), depth + 1)?;
            }
        }
        Ok(())
    }

    fn object(
        &self,
        schema: &Map<String, Value>,
        object: &Map<String, Value>,
        at: &str,
        depth: usize,
    ) -> Result<(), Misfit> {
        let empty = Map::new();
        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        // What the values hold first: the form whose own keys fit is the one the caller meant.
        for (name, value) in object {
            if let Some(form) = properties.get(name) {
                self.fits(form, value, &join(at, name), depth + 1)?;
            }
        }
        let others = schema.get("additionalProperties");
        let patterned = schema.contains_key("patternProperties");
        for (name, value) in object {
            if properties.contains_key(name) || patterned {
                continue;
            }
            match others {
                Some(Value::Bool(false)) => {
                    let known = properties.keys().cloned().collect();
                    return Err(misfit(&join(at, name), Fault::Unknown(known)));
                }
                Some(form) => self.fits(form, value, &join(at, name), depth + 1)?,
                None => {}
            }
        }
        let required = schema.get("required").and_then(Value::as_array);
        for name in required.into_iter().flatten().filter_map(Value::as_str) {
            if !object.contains_key(name) {
                return Err(misfit(&join(at, name), Fault::Missing));
            }
        }
        Ok(())
    }

    /// A reference inside the schema (`#/$defs/Name`); `None` for one elsewhere.
    fn resolve(&self, reference: &str) -> Option<&Value> {
        let pointer = reference.strip_prefix('#')?;
        let mut parts = pointer.split('/').skip(1);
        let first = parts.next()?;
        let mut at = self.root.get(&unescape(first))?;
        for part in parts {
            at = match at {
                Value::Object(object) => object.get(&unescape(part))?,
                Value::Array(items) => items.get(part.parse::<usize>().ok()?)?,
                _ => return None,
            };
        }
        Some(at)
    }
}

/// The `type` keyword: one type or a list of them.
fn typed(schema: &Map<String, Value>, value: &Value, at: &str) -> Result<(), Misfit> {
    let fits = |kind: &str| match kind {
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "string" => value.is_string(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => true,
    };
    let kinds: Vec<&str> = match schema.get("type") {
        Some(Value::String(kind)) => vec![kind.as_str()],
        Some(Value::Array(kinds)) => kinds.iter().filter_map(Value::as_str).collect(),
        _ => return Ok(()),
    };
    if kinds.is_empty() || kinds.iter().any(|kind| fits(kind)) {
        return Ok(());
    }
    Err(misfit(at, Fault::Type(kinds.join(" or "))))
}

/// The `enum` and `const` keywords.
fn listed(schema: &Map<String, Value>, value: &Value, at: &str) -> Result<(), Misfit> {
    let listed: Vec<&Value> = match (schema.get("enum"), schema.get("const")) {
        (Some(Value::Array(values)), _) => values.iter().collect(),
        (_, Some(constant)) => vec![constant],
        _ => return Ok(()),
    };
    if listed.contains(&value) {
        return Ok(());
    }
    let shown = listed
        .iter()
        .map(|value| {
            value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_owned)
        })
        .collect();
    Err(misfit(at, Fault::NotListed(shown)))
}

/// The bounds of a number, and the lengths of a string or a list.
fn bounded(schema: &Map<String, Value>, value: &Value, at: &str) -> Result<(), Misfit> {
    let bound = |key: &str| schema.get(key).and_then(Value::as_f64);
    let shown = |key: &str| schema.get(key).map(Value::to_string).unwrap_or_default();
    if let Some(number) = value.as_f64() {
        if bound("minimum").is_some_and(|minimum| number < minimum)
            || bound("exclusiveMinimum").is_some_and(|minimum| number <= minimum)
        {
            let key = if schema.contains_key("minimum") {
                "minimum"
            } else {
                "exclusiveMinimum"
            };
            return Err(misfit(at, Fault::TooSmall(shown(key))));
        }
        if bound("maximum").is_some_and(|maximum| number > maximum)
            || bound("exclusiveMaximum").is_some_and(|maximum| number >= maximum)
        {
            let key = if schema.contains_key("maximum") {
                "maximum"
            } else {
                "exclusiveMaximum"
            };
            return Err(misfit(at, Fault::TooLarge(shown(key))));
        }
    }
    let length = match value {
        Value::String(text) => Some((text.chars().count(), "minLength", "maxLength")),
        Value::Array(items) => Some((items.len(), "minItems", "maxItems")),
        _ => None,
    };
    if let Some((length, least, most)) = length {
        let length = length as f64;
        if bound(least).is_some_and(|least| length < least) {
            return Err(misfit(at, Fault::TooShort));
        }
        if bound(most).is_some_and(|most| length > most) {
            return Err(misfit(at, Fault::TooLong));
        }
    }
    Ok(())
}

fn misfit(at: &str, fault: Fault) -> Misfit {
    Misfit {
        at: at.to_owned(),
        fault,
    }
}

/// `name` below `at`, clipped to [`NAME_MAX`] characters: a key is the caller's text.
fn join(at: &str, name: &str) -> String {
    let name: String = name.chars().take(NAME_MAX).collect();
    if at.is_empty() {
        name
    } else {
        format!("{at}.{name}")
    }
}

/// A JSON pointer's segment (`~1` is `/`, `~0` is `~`).
fn unescape(segment: &str) -> String {
    segment.replace("~1", "/").replace("~0", "~")
}

#[cfg(test)]
#[path = "arguments_tests.rs"]
mod tests;
