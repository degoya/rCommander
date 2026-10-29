//! The question of an MCP call through `apply`, against an engine with a window attached: the
//! call waits until a button answers, Escape denies, the keys belong to the open question.

use std::time::Duration;

use rc_engine::{
    CommandClass, McpAnswer, McpArgument, McpAsk, McpSessionOffer, McpVerdict, PanelId,
};
use tempfile::TempDir;
use tokio::task::JoinHandle;

use super::McpConfirmMsg;
use crate::app::{Message, harness::Harness};

async fn harness() -> (TempDir, Harness) {
    let root = crate::app::harness::tree(&[("sub/", "")]);
    let harness = Harness::headless(root.path()).await;
    harness.app.engine().attach_window();
    (root, harness)
}

/// An MCP call asking about `class` in the background, then the window catching up.
async fn asking(
    harness: &mut Harness,
    class: CommandClass,
    within: Duration,
) -> JoinHandle<McpVerdict> {
    let engine = harness.app.engine().clone();
    let ask = McpAsk {
        tool: "terminal_open".to_owned(),
        class,
        arguments: vec![McpArgument {
            name: "shell".to_owned(),
            value: "cmd".to_owned(),
        }],
        panel: None,
        cursor: false,
        session: None,
    };
    let call = tokio::spawn(async move { engine.ask_mcp(ask, within).await });
    while harness.app.engine().mcp_questions().is_empty() {
        tokio::task::yield_now().await;
    }
    harness.settle().await;
    call
}

fn open_question(harness: &Harness) -> u64 {
    harness.app.mcp_questions.first().expect("a question").id
}

#[tokio::test]
async fn the_call_waits_until_the_person_clicks_allow() {
    let (_root, mut harness) = harness().await;
    let call = asking(&mut harness, CommandClass::Exec, Duration::from_secs(30)).await;
    let id = open_question(&harness);

    // A key typed as the dialog appears allows nothing.
    harness.press("Enter").await;
    harness.press("Tab").await;
    assert_eq!(harness.app.active, PanelId::Left, "the panels do not move");
    assert_eq!(harness.app.mcp_questions.len(), 1);
    assert!(!call.is_finished());

    harness
        .send(Message::McpConfirm(McpConfirmMsg::Answer(
            id,
            McpAnswer::Allow,
        )))
        .await;
    assert_eq!(call.await.expect("joins"), McpVerdict::Allowed);
    harness.settle().await;
    assert!(harness.app.mcp_questions.is_empty());
}

#[tokio::test]
async fn escape_denies_the_question() {
    let (_root, mut harness) = harness().await;
    let call = asking(
        &mut harness,
        CommandClass::Destructive,
        Duration::from_secs(30),
    )
    .await;
    harness.press("Escape").await;
    assert_eq!(call.await.expect("joins"), McpVerdict::Denied);
    assert!(harness.app.mcp_questions.is_empty());
    harness.press("Tab").await;
    assert_eq!(
        harness.app.active,
        PanelId::Right,
        "the keys are the panels' again"
    );
}

#[tokio::test]
async fn the_session_is_not_allowed_where_it_was_not_offered() {
    let (_root, mut harness) = harness().await;
    let call = asking(&mut harness, CommandClass::Exec, Duration::from_secs(30)).await;
    let id = open_question(&harness);
    harness
        .send(Message::McpConfirm(McpConfirmMsg::Answer(
            id,
            McpAnswer::AllowForSession,
        )))
        .await;
    assert_eq!(harness.app.mcp_questions.len(), 1, "still asking");
    assert!(!call.is_finished());
    call.abort();
}

#[tokio::test]
async fn a_question_that_timed_out_leaves_the_window() {
    let (_root, mut harness) = harness().await;
    let call = asking(&mut harness, CommandClass::Exec, Duration::from_millis(20)).await;
    assert_eq!(call.await.expect("joins"), McpVerdict::TimedOut);
    harness.settle().await;
    assert!(harness.app.mcp_questions.is_empty());
}

#[tokio::test]
async fn a_window_that_missed_events_reads_the_questions_again() {
    let (_root, mut harness) = harness().await;
    let engine = harness.app.engine().clone();
    let ask = McpAsk {
        tool: "ops_delete".to_owned(),
        class: CommandClass::Destructive,
        arguments: Vec::new(),
        panel: Some(PanelId::Right),
        cursor: false,
        session: None,
    };
    let call = tokio::spawn(async move { engine.ask_mcp(ask, Duration::from_secs(30)).await });
    while harness.app.engine().mcp_questions().is_empty() {
        tokio::task::yield_now().await;
    }
    harness.miss_events();
    assert!(harness.app.mcp_questions.is_empty());
    harness.send(Message::EngineLagged).await;
    assert_eq!(harness.app.mcp_questions.len(), 1);
    assert_eq!(harness.app.mcp_questions[0].panel.panel, PanelId::Right);
    call.abort();
}

/// RC-010-27: an executing tool offered for its target is allowed for the session by a click;
/// the status bar shows the grant, and a click there takes it back.
#[tokio::test]
async fn an_exec_grant_shows_in_the_status_bar_until_revoked() {
    let (_root, mut harness) = harness().await;
    let engine = harness.app.engine().clone();
    let ask = McpAsk {
        tool: "terminal_send_input".to_owned(),
        class: CommandClass::Exec,
        arguments: Vec::new(),
        panel: None,
        cursor: false,
        session: Some(McpSessionOffer::Minutes(30)),
    };
    let call = tokio::spawn(async move { engine.ask_mcp(ask, Duration::from_secs(30)).await });
    while harness.app.engine().mcp_questions().is_empty() {
        tokio::task::yield_now().await;
    }
    harness.settle().await;
    let id = open_question(&harness);
    harness
        .send(Message::McpConfirm(McpConfirmMsg::Answer(
            id,
            McpAnswer::AllowForSession,
        )))
        .await;
    assert_eq!(call.await.expect("joins"), McpVerdict::AllowedForSession);

    // What the MCP gate does with that answer.
    harness.app.engine().allow_mcp_for_session(
        "s1",
        "terminal_send_input",
        Some("terminal=1".to_owned()),
        Some(Duration::from_secs(1800)),
    );
    harness.settle().await;
    assert_eq!(harness.app.mcp_grants.len(), 1);

    harness
        .send(Message::McpConfirm(McpConfirmMsg::Revoke))
        .await;
    harness.settle().await;
    assert!(harness.app.mcp_grants.is_empty());
    assert!(harness.app.engine().mcp_grants().is_empty());
}
