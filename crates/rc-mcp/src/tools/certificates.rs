//! The `certificates` tool group: reading the FTPS certificates a person trusted (RC-070-02).
//!
//! There is deliberately no tool that trusts, accepts, answers or removes a certificate:
//! `certificates.answer` and `certificates.remove` are `CommandClass::GuiOnly`, omitted in
//! `coverage.rs` and refused by `Engine::dispatch_mcp`. A connection an agent starts to a server
//! with a certificate the system does not trust answers `certificate_unknown`,
//! `certificate_changed` or `certificate_expired` with the fingerprint, and a person decides in
//! the rCommander window.

use rc_engine::{Command, Outcome};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use crate::{
    RcMcpServer,
    answer::{ToolResult, error_result, json_result},
};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ListParams {
    /// Only certificates whose host, subject, issuer or fingerprint contains this, ignoring case.
    #[serde(default)]
    pub filter: Option<String>,
}

/// An outcome this tool does not expect (cannot happen while the engine keeps its contract), as
/// an internal error rather than a guess.
fn unexpected(outcome: &Outcome) -> ToolResult {
    tracing::warn!(
        ?outcome,
        "a certificates command answered something unexpected"
    );
    Err(rmcp::ErrorData::internal_error(
        "the command answered something this tool does not expect",
        None,
    ))
}

#[tool_router(router = certificates_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "List the FTPS server certificates rCommander trusts: id, host, port, SHA-256 fingerprint of the certificate (AB:CD:...), subject, issuer, not_after (Unix seconds), added_at, last_seen_at. filter narrows by host, subject, issuer or fingerprint. Certificates cannot be trusted, accepted or removed through MCP: a connection to a server with an untrusted, changed or expired certificate fails with certificate_unknown, certificate_changed or certificate_expired and a person decides in the rCommander window."
    )]
    pub(crate) async fn certificates_list(
        &self,
        Parameters(params): Parameters<ListParams>,
    ) -> ToolResult {
        match self
            .context
            .engine
            .dispatch_mcp(Command::ListCertificates {
                filter: params.filter,
            })
            .await
        {
            Ok(Outcome::TrustedCertificates { certificates }) => {
                json_result(&serde_json::json!({ "certificates": certificates }))
            }
            Ok(other) => unexpected(&other),
            Err(error) => Ok(error_result(error.code(), &error.to_string())),
        }
    }
}

#[cfg(test)]
#[path = "certificates_tests.rs"]
mod tests;
