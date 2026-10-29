//! The gate that refuses a secret among the arguments of a tool that takes none: the
//! connections', the shared credentials' and the login scripts' (`connections.rs`,
//! `credentials.rs`, `login_scripts.rs`).

use rmcp::model::{CallToolResult, JsonObject};
use serde_json::Value;

use crate::{answer::error_result, redact::is_secret_name};

/// The prefixes of the tools that take no secrets: the connections', the shared credentials'
/// (RC-050-12) and the login scripts'.
const GROUP_PREFIXES: &[&str] = &["connections_", "credentials_", "login_scripts_"];

/// Parts of a parameter name that make it a secret's, beside the names the mask knows.
const SECRET_PARTS: &[&str] = &["password", "passphrase", "passwd", "secret", "private_key"];

/// The refusal of a call to a tool of this group whose arguments name a secret, at any depth;
/// `None` for any other call.
pub(crate) fn refuse_secrets(tool: &str, arguments: Option<&JsonObject>) -> Option<CallToolResult> {
    if !GROUP_PREFIXES.iter().any(|prefix| tool.starts_with(prefix)) {
        return None;
    }
    let name = arguments.and_then(secret_name)?;
    Some(error_result(
        "secret_not_accepted",
        &format!(
            "{tool} takes no secrets, and the parameter {name} would carry one: passwords, \
             private keys and passphrases are entered by the person in rCommander, never \
             through MCP. Call again without it."
        ),
    ))
}

/// The first parameter name in `object`, or in an object nested in it, that names a secret.
fn secret_name(object: &JsonObject) -> Option<String> {
    object.iter().find_map(|(name, value)| {
        let lowered = name.to_ascii_lowercase();
        if is_secret_name(&lowered) || SECRET_PARTS.iter().any(|part| lowered.contains(part)) {
            return Some(name.clone());
        }
        match value {
            Value::Object(nested) => secret_name(nested),
            Value::Array(items) => items.iter().find_map(|item| match item {
                Value::Object(nested) => secret_name(nested),
                _ => None,
            }),
            _ => None,
        }
    })
}
