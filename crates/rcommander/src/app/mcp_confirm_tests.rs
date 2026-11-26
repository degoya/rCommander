//! The question of an MCP call through `apply`, against an engine with a window attached: the
//! call waits until a button answers, Escape denies, the keys belong to the open question, and
//! Enter and Space answer only once the question has been on top for a second (RC-110-80).

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
    let before = harness.app.engine().mcp_questions().len();
    let call = tokio::spawn(async move { engine.ask_mcp(ask, within).await });
    while harness.app.engine().mcp_questions().len() == before {
        tokio::task::yield_now().await;
    }
    harness.settle().await;
    call
}

fn open_question(harness: &Harness) -> u64 {
    harness.app.mcp_questions.first().expect("a question").id
}

/// Enter or Space on the button of `answer`, as its stop sends them.
fn key(id: u64, answer: McpAnswer) -> Message {
    Message::McpConfirm(McpConfirmMsg::Key(id, answer))
}

/// The second after the question `id` came on top has passed.
fn armed(id: u64) -> Message {
    Message::McpConfirm(McpConfirmMsg::Armed(id))
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

/// RC-110-80: in the question's first second Enter and Space answer nothing - not even on Deny,
/// which has the ring - and no key walks to Allow; a second later the button under the ring
/// answers, Allow included.
#[tokio::test]
async fn keys_answer_only_after_the_first_second() {
    let (_root, mut harness) = harness().await;
    let call = asking(&mut harness, CommandClass::Exec, Duration::from_secs(30)).await;
    let id = open_question(&harness);
    assert!(!harness.app.mcp_armed(), "its first second");
    harness.send(key(id, McpAnswer::Deny)).await;
    harness.send(key(id, McpAnswer::Allow)).await;
    for chord in ["Enter", "Space", "Tab"] {
        harness.press(chord).await;
    }
    assert_eq!(harness.app.mcp_questions.len(), 1, "no key answered");
    assert!(!call.is_finished());

    harness.send(armed(id)).await;
    assert!(harness.app.mcp_armed(), "a second later");
    harness.send(key(id, McpAnswer::Allow)).await;
    assert_eq!(call.await.expect("joins"), McpVerdict::Allowed);
    harness.settle().await;
    assert!(harness.app.mcp_questions.is_empty());
    assert!(!harness.app.mcp_armed());
}

/// RC-110-80: the question that follows an answered one waits its own second - a key held on
/// the first one's Allow does not answer the next one - and the first one's second is not its.
#[tokio::test]
async fn the_next_question_waits_its_own_second() {
    let (_root, mut harness) = harness().await;
    let first = asking(&mut harness, CommandClass::Exec, Duration::from_secs(30)).await;
    let second = asking(&mut harness, CommandClass::Exec, Duration::from_secs(30)).await;
    assert_eq!(harness.app.mcp_questions.len(), 2);
    let one = open_question(&harness);
    harness.send(armed(one)).await;
    harness.send(key(one, McpAnswer::Allow)).await;
    assert_eq!(first.await.expect("joins"), McpVerdict::Allowed);
    harness.settle().await;

    let two = open_question(&harness);
    assert_ne!(one, two);
    assert!(!harness.app.mcp_armed(), "the next question's first second");
    harness.send(key(two, McpAnswer::Allow)).await;
    harness.send(armed(one)).await;
    assert!(
        !harness.app.mcp_armed(),
        "the first question's second is not the next one's"
    );
    assert_eq!(harness.app.mcp_questions.len(), 1, "still asking");
    assert!(!second.is_finished());

    harness.send(armed(two)).await;
    harness.send(key(two, McpAnswer::Deny)).await;
    assert_eq!(second.await.expect("joins"), McpVerdict::Denied);
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
    // No event brought it on top: the next key starts its second (RC-110-80) and answers nothing.
    let id = open_question(&harness);
    harness.send(armed(id)).await;
    assert!(!harness.app.mcp_armed(), "no second started yet");
    harness.press("Enter").await;
    assert_eq!(harness.app.mcp_questions.len(), 1, "Enter answered nothing");
    harness.send(armed(id)).await;
    assert!(harness.app.mcp_armed(), "the key started its second");
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
