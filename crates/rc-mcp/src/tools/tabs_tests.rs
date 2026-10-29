use rc_engine::{Engine, PanelId, VfsPath};
use rmcp::{handler::server::wrapper::Parameters, model::CallToolResult};
use serde_json::Value;

use super::*;
use crate::test_support::Fixture;
use crate::tools::panels::NavigateParams;

fn body(result: &CallToolResult) -> Value {
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    serde_json::from_str(&text).expect("json")
}

/// A server over an engine on a folder with `a` and `b`.
async fn server() -> (tempfile::TempDir, RcMcpServer, Engine) {
    let fixture = Fixture::new();
    for dir in ["a", "b"] {
        std::fs::create_dir(fixture.root().join(dir)).expect("dir");
    }
    let built = fixture.build().await;
    let server = built.server();
    (built.root, server, built.engine)
}

fn left() -> Parameters<PanelParams> {
    Parameters(PanelParams {
        panel: Some(PanelArg::Left),
    })
}

/// The tab tools of the acceptance list: tab_open, tab_list, tab_close.
#[tokio::test]
async fn tabs_open_list_and_close() {
    let (_root, server, engine) = server().await;
    let opened = server
        .tab_open(Parameters(TabOpenParams {
            path: Some("a".into()),
            cursor: false,
            other_panel: false,
            panel: Some(PanelArg::Left),
        }))
        .await
        .expect("answer");
    let answer = body(&opened);
    assert_eq!(answer["kind"], "tabs", "{answer}");
    assert_eq!(answer["tabs"][1]["title"], "a", "{answer}");
    assert_eq!(answer["tabs"][1]["active"], true, "{answer}");

    let listed = body(&server.tab_list(left()).await.expect("answer"));
    assert_eq!(listed["tabs"].as_array().map(Vec::len), Some(2), "{listed}");

    let closed = server
        .tab_close(Parameters(TabParams {
            tab: None,
            panel: Some(PanelArg::Left),
        }))
        .await
        .expect("answer");
    assert_eq!(body(&closed)["tabs"].as_array().map(Vec::len), Some(1));
    assert_ne!(
        engine.panel(PanelId::Left).path.path.file_name(),
        Some("a".as_ref())
    );

    let refused = server
        .tab_close(Parameters(TabParams {
            tab: None,
            panel: Some(PanelArg::Left),
        }))
        .await
        .expect("answer");
    assert_eq!(refused.is_error, Some(true));
    assert_eq!(body(&refused)["code"], "last_tab");
}

/// history_back of the acceptance list, and history_list.
#[tokio::test]
async fn history_back_returns_to_the_folder_before() {
    let (root, server, engine) = server().await;
    server
        .navigate(Parameters(NavigateParams {
            path: "a".into(),
            panel: Some(PanelArg::Left),
        }))
        .await
        .expect("answer");
    let listed = body(&server.history_list(left()).await.expect("answer"));
    assert_eq!(listed["kind"], "history", "{listed}");
    assert_eq!(listed["current"], 1, "{listed}");

    let back = server.history_back(left()).await.expect("answer");
    assert_ne!(back.is_error, Some(true), "{:?}", body(&back));
    assert_eq!(
        engine.panel(PanelId::Left).path,
        VfsPath::local(root.path())
    );

    let refused = body(&server.history_back(left()).await.expect("answer"));
    assert_eq!(refused["code"], "history_end");
}

#[tokio::test]
async fn panel_swap_exchanges_the_folders() {
    let (root, server, engine) = server().await;
    server
        .navigate(Parameters(NavigateParams {
            path: "b".into(),
            panel: Some(PanelArg::Right),
        }))
        .await
        .expect("answer");
    server
        .panel_swap(Parameters(PanelSwapParams { with_tabs: false }))
        .await
        .expect("answer");
    assert_eq!(
        engine.panel(PanelId::Left).path,
        VfsPath::local(root.path().join("b"))
    );
}

/// tab_move puts a tab at another place of its row, the shown one stays shown (RC-010-24); an
/// unknown tab is named.
#[tokio::test]
async fn tab_move_reorders_the_row() {
    let (_root, server, _engine) = server().await;
    server
        .tab_open(Parameters(TabOpenParams {
            path: Some("a".into()),
            cursor: false,
            other_panel: false,
            panel: Some(PanelArg::Left),
        }))
        .await
        .expect("answer");
    let listed = body(&server.tab_list(left()).await.expect("answer"));
    let first = listed["tabs"][0]["id"].as_u64().expect("an id");
    let moved = body(
        &server
            .tab_move(Parameters(TabMoveParams {
                tab: first,
                to: 1,
                panel: Some(PanelArg::Left),
            }))
            .await
            .expect("answer"),
    );
    assert_eq!(moved["kind"], "tabs", "{moved}");
    assert_eq!(moved["tabs"][0]["title"], "a", "{moved}");
    assert_eq!(moved["tabs"][0]["active"], true, "{moved}");
    assert_eq!(moved["tabs"][1]["id"].as_u64(), Some(first), "{moved}");

    let refused = server
        .tab_move(Parameters(TabMoveParams {
            tab: 999,
            to: 0,
            panel: None,
        }))
        .await
        .expect("answer");
    assert_eq!(refused.is_error, Some(true));
    assert_eq!(body(&refused)["code"], "tab_not_found");
}
