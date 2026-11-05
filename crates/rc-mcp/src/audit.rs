//! The MCP audit log (RC-080-66): every tool call the gate decided is one row of `mcp_audit_log`
//! (the session, the tool, the class it was held to, the decision and the answer's code) while
//! the protected setting `mcp.audit_log` is on (the default).
//!
//! The arguments go in masked as every answer is (`mask.rs`, the known secrets included), with
//! the confirmation code left out, direction and zero-width characters spelled out, and cut to
//! [`MCP_AUDIT_ARGUMENTS_MAX_BYTES`]; the answer itself never does. A failed write is a warning,
//! never the call's failure. No tool reads the log: an agent does not see what other agents did
//! (the coverage table's `omitted` row of the settings dialog's view).

use rc_db::{MCP_AUDIT_ARGUMENTS_MAX_BYTES, NewMcpAuditEntry, cut_to_bytes};
use rc_engine::{
    CommandClass, mcp_readable as readable,
    settings::{MCP_AUDIT_LOG_KEY, stored_setting},
};
use rmcp::model::{CallToolResponse, CallToolResult};
use serde_json::{Map, Value};

use crate::{confirm::CODE_ARGUMENT, mask, server::McpContext};

/// What the gate did with a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decision {
    /// The policy allows it: it ran.
    Allowed,
    /// The answer is a question with a confirmation code (headless).
    Asked,
    /// Asked, and allowed: by the person, a grant for the session or a redeemed code.
    Confirmed,
    /// Asked, and not allowed: declined, not answered in time, nobody to ask, refused before
    /// the question.
    Declined,
    /// The policy denies its class.
    Denied,
    /// Refused before the policy: a secret in the arguments, a network share, no class, a
    /// policy that does not read.
    Refused,
    /// No such tool.
    Unknown,
}

impl Decision {
    /// The decision as the log stores it.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Asked => "asked",
            Self::Confirmed => "confirmed",
            Self::Declined => "declined",
            Self::Denied => "denied",
            Self::Refused => "refused",
            Self::Unknown => "unknown",
        }
    }
}

/// What the gate decided about one call, filled in as it goes.
#[derive(Debug)]
pub(crate) struct Audit {
    /// The class the call was held to; `None` where the gate decided before it had one.
    pub(crate) class: Option<CommandClass>,
    pub(crate) decision: Decision,
    /// `mcp.audit_log` as the gate read it with the policy; `None` where it decided before.
    pub(crate) enabled: Option<bool>,
}

impl Default for Audit {
    fn default() -> Self {
        Self {
            class: None,
            decision: Decision::Unknown,
            enabled: None,
        }
    }
}

/// Writes the row of one call, unless the log is switched off.
pub(crate) async fn record(
    context: &McpContext,
    session: &str,
    tool: &str,
    audit: &Audit,
    arguments: Option<&Map<String, Value>>,
    answer: &Result<CallToolResponse, rmcp::ErrorData>,
) {
    let enabled = match audit.enabled {
        Some(enabled) => enabled,
        None => stored_setting(&context.db, MCP_AUDIT_LOG_KEY)
            .await
            .and_then(|value| value.as_bool())
            .unwrap_or(true),
    };
    if !enabled {
        return;
    }
    let code = match audit.decision {
        Decision::Asked => "confirmation_required".to_owned(),
        _ => code_of(answer),
    };
    let entry = NewMcpAuditEntry {
        session: session.to_owned(),
        tool: readable(tool),
        class: audit.class.map(|class| class.as_str().to_owned()),
        decision: audit.decision.as_str().to_owned(),
        code,
        arguments: logged_arguments(context, arguments),
    };
    if let Err(error) = context.db.record_mcp_call(&entry).await {
        tracing::warn!(%error, "an MCP call was not written to the audit log");
    }
}

/// `ok`, the error code of a refusal or failure (`{"code": ...}`), `error` for one without, or
/// `rpc_<n>` for a JSON-RPC error.
pub(crate) fn code_of(answer: &Result<CallToolResponse, rmcp::ErrorData>) -> String {
    match answer {
        Ok(CallToolResponse::Complete(result)) if result.is_error == Some(true) => {
            error_code(result).unwrap_or_else(|| "error".to_owned())
        }
        Ok(_) => "ok".to_owned(),
        Err(error) => format!("rpc_{}", error.code.0),
    }
}

fn error_code(result: &CallToolResult) -> Option<String> {
    let text = result.content.first()?.as_text()?;
    let body: Value = serde_json::from_str(&text.text).ok()?;
    body.get("code")?.as_str().map(readable)
}

/// The arguments as the log keeps them: masked, without the code, spelled out, as JSON cut with
/// `...` at [`MCP_AUDIT_ARGUMENTS_MAX_BYTES`].
pub(crate) fn logged_arguments(
    context: &McpContext,
    arguments: Option<&Map<String, Value>>,
) -> String {
    let mut value = Value::Object(arguments.cloned().unwrap_or_default());
    if let Some(map) = value.as_object_mut() {
        map.remove(CODE_ARGUMENT);
    }
    let engine = &context.engine;
    mask::mask_value_with(&mut value, &|text: &str| engine.scrub_secrets(text));
    let text = spelled_out(value).to_string();
    if text.len() <= MCP_AUDIT_ARGUMENTS_MAX_BYTES {
        return text;
    }
    format!(
        "{}...",
        cut_to_bytes(&text, MCP_AUDIT_ARGUMENTS_MAX_BYTES - 3)
    )
}

/// `value` with every string, keys included, [`readable`]: direction and zero-width characters
/// spelled out, so a logged name cannot dress up as another, and each clipped on its own.
fn spelled_out(value: Value) -> Value {
    match value {
        Value::String(text) => Value::String(readable(&text)),
        Value::Array(items) => Value::Array(items.into_iter().map(spelled_out).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, nested)| (readable(&key), spelled_out(nested)))
                .collect(),
        ),
        other => other,
    }
}

#[cfg(test)]
#[path = "audit_tests.rs"]
mod tests;
