use rc_engine::{Engine, PanelId, VfsPath};
use rmcp::{handler::server::wrapper::Parameters, model::CallToolResult};
use serde_json::Value;

use super::*;
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

async fn server() -> (tempfile::TempDir, RcMcpServer, Engine) {
    let fixture = Fixture::new();
    std::fs::create_dir(fixture.root().join("projects")).expect("dir");
    let built = fixture.build().await;
    let server = built.server();
    (built.root, server, built.engine)
}

/// The hotlist tools of the acceptance list: hotlist_add, hotlist_list; and opening an entry.
#[tokio::test]
async fn hotlist_add_list_and_open() {
    let (root, server, engine) = server().await;
    let added = body(
        &server
            .hotlist_add(Parameters(HotlistAddParams {
                path: Some("projects".into()),
                name: None,
                target: None,
                parent_id: None,
                panel: None,
            }))
            .await
            .expect("answer"),
    );
    assert_eq!(added["kind"], "hotlist_item", "{added}");
    assert_eq!(added["item"]["name"], "projects", "{added}");
    let id = added["item"]["id"].as_i64().expect("id");

    let listed = body(&server.hotlist_list().await.expect("answer"));
    assert_eq!(listed["kind"], "hotlist", "{listed}");
    assert_eq!(listed["items"][0]["id"], id, "{listed}");

    let opened = server
        .hotlist_open(Parameters(HotlistOpenParams {
            id,
            panel: Some(PanelArg::Right),
        }))
        .await
        .expect("answer");
    assert_ne!(opened.is_error, Some(true), "{:?}", body(&opened));
    assert_eq!(
        engine.panel(PanelId::Right).path,
        VfsPath::local(root.path().join("projects"))
    );
}

#[tokio::test]
async fn a_folder_groups_and_a_delete_takes_it_whole() {
    let (_root, server, _engine) = server().await;
    let folder = body(
        &server
            .hotlist_add_folder(Parameters(HotlistFolderParams {
                name: "Work".into(),
                parent_id: None,
            }))
            .await
            .expect("answer"),
    );
    let folder_id = folder["item"]["id"].as_i64().expect("id");
    assert_eq!(folder["item"]["folder"], true, "{folder}");
    server
        .hotlist_add(Parameters(HotlistAddParams {
            path: None,
            name: Some("Here".into()),
            target: None,
            parent_id: Some(folder_id),
            panel: None,
        }))
        .await
        .expect("answer");
    let refused = body(
        &server
            .hotlist_move(Parameters(HotlistMoveParams {
                id: folder_id,
                parent_id: Some(folder_id),
                position: None,
            }))
            .await
            .expect("answer"),
    );
    assert_eq!(refused["code"], "hotlist_cycle", "{refused}");
    let deleted = body(
        &server
            .hotlist_delete(Parameters(HotlistIdParams { id: folder_id }))
            .await
            .expect("answer"),
    );
    assert_eq!(deleted["removed"], 2, "{deleted}");
}
