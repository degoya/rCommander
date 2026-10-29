//! How tools answer: JSON text on success, an `is_error` result with a stable code on failure.

use rc_engine::EngineError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Serialize;

/// What a tool method returns.
pub(crate) type ToolResult = Result<CallToolResult, rmcp::ErrorData>;

/// A success payload as pretty JSON text.
pub(crate) fn json_result(value: &impl Serialize) -> ToolResult {
    let text = serde_json::to_string_pretty(value).map_err(|error| {
        rmcp::ErrorData::internal_error(format!("failed to serialize tool result: {error}"), None)
    })?;
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

/// A failure the model can read and act on: `{"error": ..., "code": ...}`. The code is the
/// stable one, the same i18n key the GUI shows the message under.
pub(crate) fn error_result(code: &str, message: &str) -> CallToolResult {
    let body = serde_json::json!({ "error": message, "code": code });
    CallToolResult::error(vec![ContentBlock::text(body.to_string())])
}

/// An engine result as a tool answer.
pub(crate) fn respond<T: Serialize>(result: Result<T, EngineError>) -> ToolResult {
    match result {
        Ok(value) => json_result(&value),
        Err(error) => Ok(error_result(error.code(), &error.to_string())),
    }
}
