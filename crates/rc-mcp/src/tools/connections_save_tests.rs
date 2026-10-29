//! `connections_save` end to end over an in-process pipe: one call adds or changes a connection,
//! no secret goes in or comes out, and a login script makes it exec (RC-080-25).

use rc_engine::{
    Command, CommandClass, ConnectionDraft, Outcome, SaveConnectionRequest, SaveFields,
    SecretChange, SecretKind, SecretParam,
};
use rmcp::{RoleClient, ServiceExt as _, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};

use crate::test_support::Fixture;
use crate::tools::class_of as escalate;
use crate::{McpContext, RcMcpServer};

const PASSWORD: &str = "canary-mcp-save-7f3a";

struct Setup {
    _root: tempfile::TempDir,
    context: McpContext,
}

/// An engine with an unlocked vault, as the person left it.
async fn setup() -> Setup {
    let built = Fixture::new().with_created_vault().build().await;
    Setup {
        _root: built.root,
        context: built.context,
    }
}

async fn connect(context: &McpContext) -> RunningService<RoleClient, ()> {
    let (server_side, client_side) = tokio::io::duplex(64 * 1024);
    let server = RcMcpServer::new(context.clone());
    tokio::spawn(async move {
        if let Ok(running) = server.serve(server_side).await {
            let _ = running.waiting().await;
        }
    });
    ().serve(client_side).await.expect("client")
}

/// The answer's text, unparsed: what leaves the server.
async fn call(client: &RunningService<RoleClient, ()>, tool: &str, arguments: Value) -> String {
    let arguments = arguments.as_object().cloned().unwrap_or_default();
    let result = client
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments))
        .await
        .expect("call");
    result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text")
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("json")
}

async fn connection_count(context: &McpContext) -> usize {
    match context
        .engine()
        .dispatch(Command::ListConnections)
        .await
        .expect("list")
    {
        Outcome::ConnectionTree { tree } => tree.connections.len(),
        other => panic!("the tree, got {other:?}"),
    }
}

#[tokio::test]
async fn one_call_adds_a_connection_and_another_changes_it() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let added = json(
        &call(
            &client,
            "connections_save",
            json!({ "name": "web", "host": "example.org", "username": "deploy", "favourite": false }),
        )
        .await,
    );
    assert_eq!(added["name"], "web", "{added}");
    assert_eq!(added["has_password"], false);
    let id = added["id"].as_i64().expect("an id");

    let changed = json(
        &call(
            &client,
            "connections_save",
            json!({ "id": id, "notes": "prod", "favourite": true }),
        )
        .await,
    );
    assert_eq!(changed["notes"], "prod", "{changed}");
    assert_eq!(changed["username"], "deploy", "omitted fields stay");
    assert_eq!(connection_count(&setup.context).await, 1);

    let refused = json(
        &call(
            &client,
            "connections_save",
            json!({ "id": id, "folder_id": 1, "notes": "moved?" }),
        )
        .await,
    );
    assert_eq!(refused["code"], "invalid_params", "{refused}");
}

#[tokio::test]
async fn no_secret_goes_in_and_none_comes_out() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let refused = json(
        &call(
            &client,
            "connections_save",
            json!({ "name": "web", "host": "example.org", "password": PASSWORD }),
        )
        .await,
    );
    assert_eq!(refused["code"], "secret_not_accepted", "{refused}");
    assert_eq!(connection_count(&setup.context).await, 0, "nothing stored");

    // The person saves the password in the window; an agent writes it into the notes.
    let request = SaveConnectionRequest {
        fields: SaveFields::Create {
            folder_id: None,
            connection: ConnectionDraft::new("db", "db.example.org"),
        },
        secrets: vec![SecretChange::Set {
            kind: SecretKind::Password,
            value: SecretParam::from(PASSWORD),
        }],
        login_script: None,
        favourite: None,
        credential: None,
    };
    let Ok(Outcome::Connection { connection }) = setup
        .context
        .engine()
        .dispatch(Command::SaveConnection(request))
        .await
    else {
        panic!("saved in the window");
    };
    let text = call(
        &client,
        "connections_save",
        json!({ "id": connection.id.0, "notes": PASSWORD }),
    )
    .await;
    assert!(!text.contains(PASSWORD), "{text}");
    let answer = json(&text);
    assert_eq!(answer["has_password"], true, "{answer}");
}

#[test]
fn naming_a_login_script_makes_the_call_exec() {
    let arguments = |value: Value| value.as_object().cloned();
    let with = arguments(json!({ "id": 1, "login_script_id": 2 }));
    let none = arguments(json!({ "id": 1, "login_script_id": null }));
    let without = arguments(json!({ "id": 1, "notes": "x" }));
    assert_eq!(
        escalate("connections_save", with.as_ref(), CommandClass::Write),
        CommandClass::Exec
    );
    assert_eq!(
        escalate("connections_save", none.as_ref(), CommandClass::Write),
        CommandClass::Exec
    );
    assert_eq!(
        escalate("connections_save", without.as_ref(), CommandClass::Write),
        CommandClass::Write
    );
    assert_eq!(
        escalate("connections_update", with.as_ref(), CommandClass::Write),
        CommandClass::Write
    );
}
