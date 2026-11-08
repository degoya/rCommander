//! The MCP boundary of the remote edits (RC-110-01): the list reaches MCP, the person's
//! decisions do not.

use serde_json::json;

use crate::{
    RcMcpServer,
    coverage::{COVERAGE, Coverage},
    test_support::{Fixture, call, connect},
};

#[tokio::test]
async fn only_the_list_reaches_mcp() {
    let names = RcMcpServer::tool_names();
    let tools: Vec<&String> = names
        .iter()
        .filter(|name| name.starts_with("remote_edit"))
        .collect();
    assert_eq!(tools, vec!["remote_edits_list"]);
    for (id, coverage) in COVERAGE
        .iter()
        .filter(|(id, _)| id.starts_with("remote_edit."))
    {
        let listed = *id == "remote_edit.list";
        assert_eq!(matches!(coverage, Coverage::Covered(_)), listed, "{id}");
    }
}

#[tokio::test]
async fn the_list_names_no_content() {
    let built = Fixture::new().build().await;
    let client = connect(&built.context).await;
    let listed = call(&client, "remote_edits_list", json!({})).await;
    assert_eq!(listed["kind"], "remote_edits", "{listed}");
    assert_eq!(listed["edits"], json!([]), "{listed}");
}
