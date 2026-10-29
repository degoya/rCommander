//! The shared-credential tools end to end over an in-process pipe (RC-050-12): an agent adds,
//! renames, moves and lists entries, and no secret of one goes in or comes out.

use rc_engine::{
    Command, CredentialDraft, CredentialFields, Outcome, SaveCredentialRequest, SecretChange,
    SecretKind, SecretParam,
};
use rmcp::{RoleClient, ServiceExt as _, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};

use crate::test_support::Fixture;
use crate::{McpContext, RcMcpServer};

const PASSWORD: &str = "canary-mcp-credential-91c2";

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

#[tokio::test]
async fn an_agent_adds_renames_moves_and_lists_an_entry_without_its_secret() {
    let built = Fixture::new().with_created_vault().build().await;
    let context = built.context;
    // The person stored the password in the window.
    let stored = context
        .engine()
        .dispatch(Command::SaveCredential(SaveCredentialRequest {
            fields: CredentialFields::Create {
                folder_id: None,
                credential: CredentialDraft {
                    name: "ops".into(),
                    username: Some("deploy".into()),
                },
            },
            secrets: vec![SecretChange::Set {
                kind: SecretKind::Password,
                value: SecretParam::from(PASSWORD),
            }],
        }))
        .await
        .expect("stored");
    let Outcome::Credential { credential } = stored else {
        panic!("a credential");
    };
    let client = connect(&context).await;

    let added = json(
        &call(
            &client,
            "credentials_save",
            json!({ "name": "lab", "username": "admin" }),
        )
        .await,
    );
    assert_eq!(added["name"], "lab", "{added}");
    assert_eq!(added["has_password"], false);
    let renamed = json(
        &call(
            &client,
            "credentials_save",
            json!({ "id": credential.id.0, "name": "operations" }),
        )
        .await,
    );
    assert_eq!(renamed["name"], "operations", "{renamed}");
    assert_eq!(renamed["has_password"], true);
    let user = json(
        &call(
            &client,
            "credentials_save",
            json!({ "id": credential.id.0, "username": "root" }),
        )
        .await,
    );
    assert_eq!(user["code"], "gui_only", "{user}");
    let secret = json(
        &call(
            &client,
            "credentials_save",
            json!({ "name": "x", "password": "agent-chosen" }),
        )
        .await,
    );
    assert_eq!(secret["code"], "secret_not_accepted", "{secret}");
    let folder = json(
        &call(
            &client,
            "connections_folder_create",
            json!({ "name": "lab" }),
        )
        .await,
    );
    let moved = json(
        &call(
            &client,
            "credentials_move",
            json!({ "id": credential.id.0, "folder_id": folder["id"] }),
        )
        .await,
    );
    assert_eq!(moved["folder_id"], folder["id"], "{moved}");

    let listed = call(&client, "connections_list", json!({})).await;
    assert!(!listed.contains(PASSWORD), "{listed}");
    let listed = json(&listed);
    assert_eq!(listed["credentials"][0]["name"], "lab", "{listed}");
    assert_eq!(
        listed["folders"][0]["credentials"][0]["name"], "operations",
        "{listed}"
    );
}
