//! The codes the MCP server answers with itself, beside the engine's (`EngineError::code`)
//! that tool errors pass through.

/// Every code of the server's own refusals and errors: the HTTP guard's (`http.rs`), the gate's
/// (`server.rs`) and the tools' parameter checks. The GUI keys its messages on them like on
/// the engine's codes; `codes_tests.rs` fails when a source uses a code missing here.
pub const ALL_CODES: &[&str] = &[
    "unauthorized",
    "nonce_required",
    "host_rejected",
    "origin_rejected",
    "policy_denied",
    "policy_unavailable",
    "tool_unclassified",
    "invalid_params",
    "secret_not_accepted",
    "cancelled",
    "sensitive_path",
    "network_path",
    "protected_target",
    "denied_by_user",
    "timed_out",
    "headless_ask_denied",
    "too_many_questions",
    "answer_unmaskable",
    // Engine codes a tool answers before the engine is asked: an archive test without archives
    // (`OpsError`), a property argument the tool cannot take (`EngineError`).
    "no_sources",
    "property_invalid",
];

#[cfg(test)]
#[path = "codes_tests.rs"]
mod tests;
