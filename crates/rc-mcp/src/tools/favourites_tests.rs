use rc_engine::{Command, ConnectionDraft, Outcome};
use rmcp::{handler::server::wrapper::Parameters, model::CallToolResult};
use serde_json::Value;

use super::SetFavouriteParams;
use crate::test_support::Fixture;

fn body(result: &CallToolResult) -> Value {
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    serde_json::from_str(&text).expect("json")
}

#[tokio::test]
async fn an_agent_marks_a_server_of_the_drive_bar() {
    let built = Fixture::new().build().await;
    let id = match built
        .engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: ConnectionDraft::new("web", "example.org"),
        })
        .await
        .expect("created")
    {
        Outcome::Connection { connection } => connection.id.0,
        other => panic!("a connection, got {other:?}"),
    };
    let server = built.server();

    let hidden = server
        .connections_set_favourite(Parameters(SetFavouriteParams {
            id,
            favourite: false,
        }))
        .await
        .expect("answer");
    let answer = body(&hidden);
    assert_eq!(answer["kind"], "drives", "{answer}");
    let servers = answer["drives"]
        .as_array()
        .expect("drives")
        .iter()
        .filter(|drive| drive["group"] == "servers")
        .map(|drive| drive["favourite"].as_bool().unwrap_or(false))
        .collect::<Vec<_>>();
    // Still listed, for its folder's menu - but no button of its own.
    assert_eq!(servers, [false], "{answer}");

    let unknown = server
        .connections_set_favourite(Parameters(SetFavouriteParams {
            id: 99,
            favourite: true,
        }))
        .await
        .expect("answer");
    assert_eq!(body(&unknown)["code"], "connection_not_found");
}
