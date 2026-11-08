//! The MCP boundary of the port forwards (RC-110-03): listing, stopping and deleting reach MCP,
//! saving and starting do not, and a start through the engine's MCP door opens no socket.

use rc_engine::{Command, ConnectionDraft, ForwardDraft, Outcome, SaveForwardRequest};
use serde_json::json;

use crate::{
    RcMcpServer,
    coverage::{COVERAGE, Coverage},
    test_support::{Built, Fixture, call, connect},
};

#[tokio::test]
async fn listing_stopping_and_deleting_reach_mcp_and_starting_does_not() {
    let names = RcMcpServer::tool_names();
    let tools: Vec<&String> = names
        .iter()
        .filter(|name| name.starts_with("forwards_"))
        .collect();
    assert_eq!(
        tools,
        vec![
            "forwards_delete",
            "forwards_list",
            "forwards_stop",
            "forwards_stop_all"
        ]
    );
    for (id, coverage) in COVERAGE
        .iter()
        .filter(|(id, _)| id.starts_with("forwards."))
    {
        let gui_only = ["forwards.save", "forwards.start"].contains(id);
        assert_eq!(matches!(coverage, Coverage::Omitted(_)), gui_only, "{id}");
    }
}

#[tokio::test]
async fn an_agent_lists_and_deletes_a_forward_it_could_not_have_started() {
    let Built {
        root: _root,
        engine,
        context,
        ..
    } = Fixture::new().build().await;
    let Ok(Outcome::Connection { connection }) = engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: ConnectionDraft::new("db-box", "db.example.org"),
        })
        .await
    else {
        panic!("a connection");
    };
    let port = {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        listener.local_addr().expect("address").port()
    };
    engine
        .dispatch(Command::SaveForward(SaveForwardRequest {
            connection_id: connection.id,
            id: None,
            forward: ForwardDraft {
                bind_address: "127.0.0.1".to_owned(),
                bind_port: i64::from(port),
                target_host: "db".to_owned(),
                target_port: 5432,
                auto_start: false,
                name: Some("prod".to_owned()),
            },
        }))
        .await
        .expect("saved by the person");
    let client = connect(&context).await;
    let listed = call(
        &client,
        "forwards_list",
        json!({ "connection_id": connection.id.0 }),
    )
    .await;
    let forward = &listed["forwards"][0];
    assert_eq!(forward["target_host"], "db", "{listed}");
    assert_eq!(forward["running"], false);
    let id = forward["id"].as_i64().expect("an id");
    let refused = engine
        .dispatch_mcp(Command::StartForward { id })
        .await
        .expect_err("a person's decision");
    assert_eq!(refused.code(), "gui_only");
    std::net::TcpListener::bind(("127.0.0.1", port)).expect("no socket was opened");
    let stopped = call(&client, "forwards_stop_all", json!({})).await;
    assert_eq!(stopped["stopped"], json!([]), "{stopped}");
    let deleted = call(&client, "forwards_delete", json!({ "id": id })).await;
    assert_eq!(deleted["was_running"], false, "{deleted}");
    let gone = call(&client, "forwards_delete", json!({ "id": id })).await;
    assert_eq!(gone["code"], "forward_not_found", "{gone}");
}
