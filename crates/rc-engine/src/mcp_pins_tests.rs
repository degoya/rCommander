//! RC-080-34: what a question froze of a panel, and an allowed call that finds the panel
//! elsewhere refused with `panel_changed`.

use rc_ops::{DeleteMode, ErrorPolicy};

use super::{McpPin, with_mcp_pins};
use crate::test_support;
use crate::{Command, CursorTarget, DeleteRequest, Engine, EngineError, PanelId};

async fn select(engine: &Engine, names: &[&str]) {
    engine
        .dispatch(Command::ToggleSelection {
            panel: None,
            names: names.iter().map(|name| (*name).to_owned()).collect(),
        })
        .await
        .expect("selects");
}

async fn cursor_on(engine: &Engine, name: &str) {
    engine
        .dispatch(Command::SetCursor {
            panel: None,
            target: CursorTarget::Name(name.to_owned()),
        })
        .await
        .expect("sets the cursor");
}

fn delete(sources: &[&str]) -> Command {
    Command::OpsDelete(DeleteRequest {
        panel: Some(PanelId::Left),
        sources: sources.iter().map(|name| (*name).to_owned()).collect(),
        mode: DeleteMode::Permanent,
        on_error: ErrorPolicy::Skip,
        queue: None,
    })
}

#[tokio::test]
async fn the_frozen_panel_holds_the_selection_else_the_cursor_in_full() {
    let entries: Vec<(String, String)> = (0..25)
        .map(|at| (format!("f{at:02}.txt"), "x".to_owned()))
        .collect();
    let entries: Vec<(&str, &str)> = entries
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    let (_root, engine) = test_support::engine(&entries).await;
    cursor_on(&engine, "f03.txt").await;

    let frozen = engine.freeze_mcp_panel(None, false);
    assert_eq!(frozen.names, ["f03.txt"], "nothing selected: the cursor");
    assert!(!frozen.context.selection);
    assert_eq!(frozen.path, engine.panel(PanelId::Left).path);
    assert_eq!(frozen.other, engine.panel(PanelId::Right).path);

    let all: Vec<String> = (0..25).map(|at| format!("f{at:02}.txt")).collect();
    let all_names: Vec<&str> = all.iter().map(String::as_str).collect();
    select(&engine, &all_names).await;
    let frozen = engine.freeze_mcp_panel(Some(PanelId::Left), false);
    assert_eq!(
        frozen.names, all,
        "every selected name, not only the shown ones"
    );
    assert!(frozen.context.selection);
    assert_eq!(frozen.context.entries.len(), 20);
    assert_eq!(frozen.context.more, 5);

    let frozen = engine.freeze_mcp_panel(None, true);
    assert_eq!(frozen.names, ["f03.txt"], "file_open takes the cursor");
    assert_eq!(frozen.context.entries, ["f03.txt"]);
}

#[tokio::test]
async fn a_pinned_panel_in_another_folder_refuses_the_call() {
    let (root, engine) = test_support::engine(&[("a.txt", "a"), ("docs/a.txt", "inner")]).await;
    let pins = vec![McpPin {
        panel: PanelId::Left,
        path: engine.panel(PanelId::Left).path,
    }];
    engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Left),
            path: "docs".to_owned(),
        })
        .await
        .expect("navigates");

    let refused = with_mcp_pins(pins, engine.dispatch_mcp(delete(&["a.txt"]))).await;
    assert!(
        matches!(refused, Err(EngineError::PanelChanged { panel: "left" })),
        "{refused:?}"
    );
    assert_eq!(
        refused.map_err(|error| error.code()).err(),
        Some("panel_changed")
    );
    assert!(root.path().join("a.txt").exists(), "nothing deleted above");
    assert!(
        root.path().join("docs/a.txt").exists(),
        "nor in the new folder"
    );
}

#[tokio::test]
async fn a_pin_that_still_holds_lets_the_call_run_and_only_inside_its_scope() {
    let (root, engine) = test_support::engine(&[("a.txt", "a"), ("b.txt", "b")]).await;
    let pins = vec![McpPin {
        panel: PanelId::Left,
        path: engine.panel(PanelId::Left).path,
    }];
    let outcome = with_mcp_pins(pins, engine.dispatch_mcp(delete(&["a.txt"])))
        .await
        .expect("the panel is where it was");
    let crate::Outcome::Job(job) = outcome else {
        panic!("a job: {outcome:?}");
    };
    engine.ops().wait(job.id).await.expect("the job ends");
    assert!(!root.path().join("a.txt").exists());

    // Outside a pinned call nothing is checked: the window's own commands.
    engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Left),
            path: "..".to_owned(),
        })
        .await
        .expect("navigates");
    assert!(engine.check_mcp_pins().is_ok());
    assert!(root.path().join("b.txt").exists());
}

/// The commands that resolve names against a panel check the pin under the lock they read the
/// folder with, not only `dispatch_mcp` before them.
#[tokio::test]
async fn the_commands_check_the_pin_where_they_read_the_folder() {
    let (root, engine) = test_support::engine(&[("a.txt", "a"), ("docs/a.txt", "inner")]).await;
    let pins = vec![McpPin {
        panel: PanelId::Left,
        path: engine.panel(PanelId::Left).path,
    }];
    engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Left),
            path: "docs".to_owned(),
        })
        .await
        .expect("navigates");
    let refused = with_mcp_pins(
        pins.clone(),
        engine.ops_delete(
            Some(PanelId::Left),
            &["a.txt".to_owned()],
            (DeleteMode::Permanent, ErrorPolicy::Skip),
            None,
        ),
    )
    .await;
    assert!(
        matches!(refused, Err(EngineError::PanelChanged { .. })),
        "{refused:?}"
    );
    let opened = with_mcp_pins(pins, engine.open_file(Some(PanelId::Left), Some("a.txt"))).await;
    assert!(
        matches!(opened, Err(EngineError::PanelChanged { .. })),
        "{opened:?}"
    );
    assert!(root.path().join("docs/a.txt").exists());
}

/// RC-080-42: a call that judges where it writes resolves against the folders pinned when it
/// began, the command finds them so or fails with `panel_changed`; a pin a question set stays.
#[tokio::test]
async fn a_guarded_call_resolves_against_the_folders_pinned() {
    let (_root, engine) = test_support::engine(&[("sub/a.txt", "a"), ("b.txt", "b")]).await;
    let left = engine.panel(PanelId::Left).path;
    engine
        .pin_mcp_panels(async {
            assert_eq!(
                engine.resolve_target(Some(PanelId::Left), "x"),
                left.join("x")
            );
            // The panel moves while the guard judges.
            engine
                .dispatch(Command::Navigate {
                    panel: Some(PanelId::Left),
                    path: "sub".into(),
                })
                .await
                .expect("navigates");
            assert_eq!(
                engine.resolve_target(Some(PanelId::Left), "x"),
                left.join("x"),
                "still the folder pinned"
            );
            assert_eq!(engine.mcp_folder(PanelId::Left), left);
            let refused = engine
                .dispatch_mcp(Command::OpsMkdir {
                    panel: Some(PanelId::Left),
                    path: "x".into(),
                })
                .await
                .expect_err("the panel moved");
            assert!(
                matches!(refused, EngineError::PanelChanged { panel: "left" }),
                "{refused:?}"
            );
        })
        .await;
    assert!(!left.join("x").path.exists() && !left.join("sub/x").path.exists());
    assert_eq!(engine.mcp_folder(PanelId::Left), left.join("sub"));
    let asked = vec![McpPin {
        panel: PanelId::Left,
        path: left.clone(),
    }];
    with_mcp_pins(
        asked,
        engine.pin_mcp_panels(async {
            assert_eq!(engine.mcp_folder(PanelId::Left), left, "the question's pin");
        }),
    )
    .await;
}
