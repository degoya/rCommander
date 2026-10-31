//! The MCP server: one [`RcMcpServer`] per session over a shared [`McpContext`].
//!
//! Every tool call passes one gate and one mask in [`RcMcpServer::call_tool`]: the tool's class
//! (from the command it wraps) is held against the policy, a confirmation code is asked for
//! where the policy says so, and the answer - refusals included - leaves through the credential
//! mask. In one place rather than in each tool, so no tool can forget either.

use std::{collections::HashMap, sync::Arc, time::Duration};

use rc_db::Db;
use rc_engine::{CommandClass, Engine, spec};
use rmcp::{
    RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, tool::ToolCallContext},
    model::{
        CallToolRequestParams, CallToolResponse, Implementation, ServerCapabilities, ServerConfig,
    },
    service::RequestContext,
    tool_handler,
};

use crate::{
    answer::error_result,
    ask::Gate,
    confirm::Confirmations,
    coverage::{COVERAGE, Coverage},
    gate_settings::GateSettings,
    mask,
    policy::Rule,
    tools::{connections::refuse_secrets, with_call_cancel},
};

/// What the server tells an agent at the start, kept as text beside the code: every tool it
/// names stands in backticks, and `server_tests` holds those against [`RcMcpServer::tool_names`]
/// so a renamed or removed tool cannot linger here (RC-080-54).
const INSTRUCTIONS: &str = include_str!("instructions.md");

/// What every session of one server shares: the engine, the database, the open questions and
/// the class of each tool.
#[derive(Clone)]
pub struct McpContext {
    pub(crate) engine: Engine,
    pub(crate) db: Db,
    pub(crate) confirmations: Confirmations,
    pub(crate) classes: Arc<HashMap<String, CommandClass>>,
    /// How long a call waits for the person in the window.
    pub(crate) answer_within: Duration,
}

impl McpContext {
    /// The context of a server over `engine`, reading its settings from `db`. The engine gets
    /// `db` too when it has none yet (its trust store lives there); one it has stays. Its MCP
    /// dispatches run under the sensitive-path guard from here on: a command that reads files
    /// it finds by itself (a content search, a folder's checksums) never reads one of those.
    #[must_use]
    pub fn new(engine: Engine, db: Db) -> Self {
        let _ = engine.attach_db(db.clone());
        let _ = engine.attach_read_judge(crate::guard::sensitive_path);
        let classes = COVERAGE
            .iter()
            .filter_map(|(id, coverage)| match coverage {
                Coverage::Covered(tool) => spec(id).map(|spec| ((*tool).to_owned(), spec.class)),
                Coverage::Omitted(_) => None,
            })
            // A tool that covers two commands is held to the stricter of their classes.
            .fold(HashMap::new(), |mut classes, (tool, class)| {
                classes
                    .entry(tool)
                    .and_modify(|held: &mut CommandClass| *held = held.stricter(class))
                    .or_insert(class);
                classes
            });
        Self {
            engine,
            db,
            confirmations: Confirmations::default(),
            classes: Arc::new(classes),
            answer_within: crate::ask::ANSWER_WITHIN,
        }
    }

    /// The engine the tools act on.
    #[must_use]
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Lets calls wait `within` for the person; short in the tests.
    #[cfg(test)]
    pub(crate) fn answer_within(&mut self, within: Duration) {
        self.answer_within = within;
    }

    /// Gives `tool` a class; for a test tool that wraps no command.
    #[cfg(test)]
    pub(crate) fn classify(&mut self, tool: &str, class: CommandClass) {
        Arc::make_mut(&mut self.classes).insert(tool.to_owned(), class);
    }
}

/// One MCP session's server.
#[derive(Clone)]
pub struct RcMcpServer {
    pub(crate) context: McpContext,
    tool_router: ToolRouter<Self>,
    /// The session key where the transport names none (stdio, an in-process pipe).
    instance: String,
}

impl RcMcpServer {
    /// A server over `context` with every tool.
    #[must_use]
    pub fn new(context: McpContext) -> Self {
        Self::with_router(context, Self::router())
    }

    pub(crate) fn with_router(context: McpContext, tool_router: ToolRouter<Self>) -> Self {
        Self {
            context,
            tool_router,
            instance: format!("instance:{}", crate::confirm::random_hex(8)),
        }
    }

    /// Every tool this server offers.
    pub(crate) fn router() -> ToolRouter<Self> {
        Self::panels_router()
            + Self::connections_router()
            + Self::connections_save_router()
            + Self::credentials_router()
            + Self::database_router()
            + Self::diagnostics_router()
            + Self::ssh_import_router()
            + Self::tc_import_router()
            + Self::rdm_import_router()
            + Self::hostkeys_router()
            + Self::certificates_router()
            + Self::ops_router()
            + Self::settings_router()
            + Self::terminal_router()
            + Self::vault_router()
            + Self::secret_sources_router()
            + Self::wsl_router()
            + Self::agent_router()
            + Self::files_search_router()
            + Self::files_read_router()
            + Self::compare_router()
            + Self::checksum_router()
            + Self::file_list_router()
            + Self::drives_router()
            + Self::multi_rename_router()
            + Self::login_scripts_router()
            + Self::sessions_router()
            + Self::remote_router()
            + Self::tabs_router()
            + Self::hotlist_router()
            + Self::panel_filter_router()
            + Self::flat_router()
            + Self::archives_router()
            + Self::archive_ops_router()
            + Self::properties_router()
            + Self::plugins_router()
            + Self::favourites_router()
            + Self::ssh_exec_router()
            + Self::queue_router()
            + Self::app_router()
            + Self::update_router()
            + Self::theme_router()
    }

    /// The names of every tool, sorted: what the coverage test holds the table against.
    #[must_use]
    pub fn tool_names() -> Vec<String> {
        let mut names: Vec<String> = Self::router()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.into_owned())
            .collect();
        names.sort();
        names
    }

    /// The session a question belongs to: the transport's MCP session id over HTTP (one key
    /// for every stateless HTTP request, which share the one token), this instance otherwise.
    fn session(&self, context: &RequestContext<RoleServer>) -> String {
        match context.extensions.get::<axum::http::request::Parts>() {
            Some(parts) => parts
                .headers
                .get("mcp-session-id")
                .and_then(|value| value.to_str().ok())
                .map_or_else(|| "http".to_owned(), |id| format!("http:{id}")),
            None => self.instance.clone(),
        }
    }

    /// The policy and the confirmation, then the call itself; before the mask. A call the
    /// person allowed in the window runs with what the question showed: the arguments it froze
    /// and the panels it read pinned (`freeze.rs`, RC-080-34).
    async fn call_gated(
        &self,
        mut request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, rmcp::ErrorData> {
        let tool = request.name.to_string();
        // The client's cancel of this call reaches the engine command (RC-080-50).
        let cancel = context.ct.clone();
        let mut frozen = None;
        if self.tool_router.has_route(&tool) {
            if let Some(refusal) = refuse_secrets(&tool, request.arguments.as_ref()) {
                return Ok(refusal.into());
            }
            // A share on another machine, before anything opens it (RC-080-10).
            let arguments = request.arguments.as_ref();
            if let Some(refusal) =
                crate::unc::refuse_network(&self.context.engine, &tool, arguments).await
            {
                return Ok(refusal.into());
            }
            let Some(&command_class) = self.context.classes.get(&tool) else {
                // Cannot happen while the coverage test passes; fails closed anyway.
                return Ok(error_result(
                    "tool_unclassified",
                    &format!("no command class is defined for the tool {tool}"),
                )
                .into());
            };
            // A call's arguments can make it more dangerous than its command (an overwrite):
            // the engine judges the typed command (RC-080-42).
            let class = crate::tools::effective_class(
                &self.context.engine,
                &tool,
                request.arguments.as_ref(),
                command_class,
            )
            .await;
            // The policy and what an answer needs, in one read (RC-080-54).
            let settings = match GateSettings::load(&self.context.db).await {
                Ok(settings) => settings,
                Err(error) => {
                    tracing::warn!(%error, "the MCP policy cannot be read");
                    return Ok(error_result(
                        "policy_unavailable",
                        "the MCP policy setting cannot be read",
                    )
                    .into());
                }
            };
            match settings.policy.decide(class) {
                Rule::Allowed => {}
                Rule::Deny => {
                    return Ok(error_result(
                        "policy_denied",
                        &format!(
                            "the {} tools are switched off in rCommander's MCP settings",
                            class.as_str()
                        ),
                    )
                    .into());
                }
                Rule::Ask => {
                    // The person in the window decides; headless, a code where allowed.
                    let session = self.session(&context);
                    let arguments = request.arguments.as_ref();
                    let asked = crate::ask::ask(
                        &self.context,
                        &settings,
                        &session,
                        &tool,
                        class,
                        arguments,
                    );
                    match asked.await? {
                        Gate::Answer(answer) => return Ok(answer.into()),
                        Gate::Run(run) => frozen = run,
                    }
                }
            }
        }
        let Some(frozen) = frozen else {
            let call = ToolCallContext::new(self, request, context);
            return with_call_cancel(cancel, self.tool_router.call(call)).await;
        };
        request.arguments = Some(frozen.arguments);
        let call = ToolCallContext::new(self, request, context);
        let run = with_call_cancel(cancel, self.tool_router.call(call));
        rc_engine::with_mcp_pins(frozen.pins, run).await
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for RcMcpServer {
    /// One gate and one mask for every tool; replaces the macro's passthrough and keeps its
    /// `list_tools`.
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, rmcp::ErrorData> {
        let engine = &self.context.engine;
        mask::mask_response(self.call_gated(request, context).await, &|text: &str| {
            engine.scrub_secrets(text)
        })
    }

    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(INSTRUCTIONS);
        let mut implementation = Implementation::default();
        implementation.name = "rcommander".to_owned();
        implementation.title = Some("rCommander".to_owned());
        implementation.version = env!("CARGO_PKG_VERSION").to_owned();
        info.server_info = implementation;
        info
    }
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
