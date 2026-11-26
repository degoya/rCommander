//! What an agent sends `rcommander mcp` over stdio: the bytes framed into JSON-RPC messages by
//! rmcp's stdio transport as `serve_io` reads them (one message per line, a line it cannot parse
//! answered or dropped), and the arguments of every `tools/call` among them held against the
//! tool's input schema as the server does before the gate (`rc_mcp::check_arguments`,
//! RC-110-57). A refusal is a tool error, never a panic.

#![no_main]

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use libfuzzer_sys::fuzz_target;
use rc_mcp::{RcMcpServer, check_arguments};
use rmcp::{
    RoleServer,
    model::{ClientRequest, JsonObject, JsonRpcMessage},
    transport::{Transport as _, async_rw::AsyncRwTransport},
};
use tokio::runtime::Runtime;

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a current-thread runtime builds")
    })
}

/// Every tool's input schema by its name, as the server's router lists them.
fn schemas() -> &'static HashMap<String, Arc<JsonObject>> {
    static SCHEMAS: OnceLock<HashMap<String, Arc<JsonObject>>> = OnceLock::new();
    SCHEMAS.get_or_init(|| {
        RcMcpServer::tools()
            .into_iter()
            .map(|tool| (tool.name.into_owned(), tool.input_schema))
            .collect()
    })
}

async fn serve(data: &[u8]) {
    // The answers the transport writes itself (an invalid request's error) go nowhere.
    let mut transport =
        AsyncRwTransport::<RoleServer, &[u8], tokio::io::Sink>::new_server(data, tokio::io::sink());
    while let Some(message) = transport.receive().await {
        let JsonRpcMessage::Request(request) = message else {
            continue;
        };
        let ClientRequest::CallToolRequest(call) = request.request else {
            continue;
        };
        let tool: &str = &call.params.name;
        let Some(schema) = schemas().get(tool) else {
            continue;
        };
        if let Err(refusal) = check_arguments(tool, schema, call.params.arguments.as_ref()) {
            assert_eq!(
                refusal.is_error,
                Some(true),
                "the refusal of a call to {tool} is no tool error"
            );
        }
    }
}

fuzz_target!(|data: &[u8]| {
    runtime().block_on(serve(data));
});
