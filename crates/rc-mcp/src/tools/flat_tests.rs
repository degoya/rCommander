//! `panel_branch_view` and `panel_feed_paths` over an in-process pipe: the panel turns flat and
//! back, and fed paths land in it.

use std::{fs, path::MAIN_SEPARATOR, time::Duration};

use rc_engine::{Engine, PanelId, VfsPath};
use serde_json::json;
use tempfile::TempDir;

use crate::McpContext;
use crate::test_support::{Fixture, call, connect};

/// `root/` with `a.txt` and `sub/b.txt`.
async fn context() -> (McpContext, TempDir) {
    let fixture = Fixture::new();
    let root = fixture.root();
    fs::create_dir(root.join("sub")).expect("mkdir");
    fs::write(root.join("a.txt"), "a").expect("write");
    fs::write(root.join("sub/b.txt"), "b").expect("write");
    let built = fixture.build().await;
    (built.context, built.root)
}

/// The left panel's entry names once its flat view has read everything.
async fn settled(engine: &Engine) -> Vec<String> {
    for _ in 0..500 {
        let state = engine.panel(PanelId::Left);
        if state.flat.as_ref().is_some_and(|flat| !flat.loading) {
            return state
                .entries
                .iter()
                .map(|entry| entry.name.clone())
                .collect();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the flat view did not finish");
}

#[tokio::test]
async fn the_branch_view_turns_the_panel_flat_and_back() {
    let (context, root) = context().await;
    let client = connect(&context).await;
    let state = call(&client, "panel_branch_view", json!({})).await;
    assert_eq!(state["flat"]["source"], "branch");
    let names = settled(&context.engine).await;
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.contains(&"a.txt".to_owned()));

    let state = call(&client, "panel_branch_view", json!({ "show": false })).await;
    assert!(state.get("flat").is_none(), "{state}");
    assert_eq!(
        context.engine.panel(PanelId::Left).path,
        VfsPath::local(root.path()),
        "the cursor stood on a.txt, a file of the panel's own folder"
    );
}

#[tokio::test]
async fn fed_paths_show_in_the_panel() {
    let (context, _root) = context().await;
    let client = connect(&context).await;
    let state = call(
        &client,
        "panel_feed_paths",
        json!({ "paths": ["sub/b.txt", "missing.txt"] }),
    )
    .await;
    assert_eq!(state["flat"]["source"], "paths");
    let names = settled(&context.engine).await;
    assert_eq!(names, [format!("sub{MAIN_SEPARATOR}b.txt")]);
    let flat = context.engine.panel(PanelId::Left).flat.expect("flat");
    assert_eq!(flat.skipped, 1);

    let refused = call(
        &client,
        "panel_feed_paths",
        json!({ "paths": [], "root": "a.txt" }),
    )
    .await;
    assert_eq!(refused["code"], "not_a_directory", "{refused}");
}
