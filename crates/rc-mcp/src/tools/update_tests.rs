use rmcp::model::CallToolResult;
use serde_json::Value;

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

/// The status names the running version and never anything that identifies the installation.
#[tokio::test]
async fn update_status_answers_the_running_version() {
    let built = Fixture::new().build().await;
    let server = built.server();
    let result = server.update_status().await.expect("answer");
    assert_ne!(result.is_error, Some(true));
    let answer = body(&result);
    assert_eq!(answer["kind"], "update");
    assert_eq!(answer["running_version"], env!("CARGO_PKG_VERSION"));
    assert!(answer.get("installation_id").is_none(), "{answer}");
}

/// Installing and downloading have no tool: replacing the program is a person's decision.
#[test]
fn no_tool_installs_or_downloads_an_update() {
    let names = crate::RcMcpServer::tool_names();
    assert!(names.contains(&"update_status".to_owned()));
    assert!(names.contains(&"update_check".to_owned()));
    assert!(
        !names
            .iter()
            .any(|name| name.contains("install") && name.starts_with("update"))
    );
    assert!(
        !names
            .iter()
            .any(|name| name.contains("download") && name.starts_with("update"))
    );
}
