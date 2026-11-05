//! The MCP page's call log (RC-080-66) through its `update`, against a database: read when
//! shown, filtered by session, refreshed, cleared - and an answer for another filter dropped.

use rc_db::{Db, NewMcpAuditEntry};

use super::{McpLogCall, McpLogMsg, McpLogPage, SessionChoice, Step};
use crate::app::settings::mcp_log_view::{decision_title, entry_line};

fn entry(session: &str, tool: &str, decision: &str) -> NewMcpAuditEntry {
    NewMcpAuditEntry {
        session: session.to_owned(),
        tool: tool.to_owned(),
        class: Some("read".to_owned()),
        decision: decision.to_owned(),
        code: "ok".to_owned(),
        arguments: r#"{"panel":"left"}"#.to_owned(),
    }
}

async fn db() -> Db {
    let db = Db::open_in_memory().await.expect("db");
    for (session, tool, decision) in [
        ("http:a", "navigate", "allowed"),
        ("http:b", "ops_delete", "denied"),
        ("http:a", "terminal_read_screen", "confirmed"),
    ] {
        db.record_mcp_call(&entry(session, tool, decision))
            .await
            .expect("record");
    }
    db
}

/// Runs the call a step asks for and feeds its answer back; the next step.
async fn answered(page: &mut McpLogPage, db: &Db, step: Step) -> Step {
    let call = step.call.expect("a database call");
    let answer = call.run(db.clone()).await;
    page.update(answer)
}

fn tools(page: &McpLogPage) -> Vec<&str> {
    page.entries
        .as_deref()
        .expect("read")
        .iter()
        .map(|entry| entry.tool.as_str())
        .collect()
}

#[tokio::test]
async fn the_log_is_read_newest_first_and_filtered_by_session() {
    let db = db().await;
    let mut page = McpLogPage::default();
    let call = page.read();
    assert!(page.busy);
    assert_eq!(call, McpLogCall::Read { session: None });
    let step = page.update(call.run(db.clone()).await);
    assert_eq!(step, Step::default());
    assert!(!page.busy);
    assert_eq!(
        tools(&page),
        ["terminal_read_screen", "ops_delete", "navigate"]
    );
    assert_eq!(page.sessions, ["http:a", "http:b"]);

    let step = page.update(McpLogMsg::Filter(SessionChoice(Some("http:b".to_owned()))));
    assert_eq!(
        step.call,
        Some(McpLogCall::Read {
            session: Some("http:b".to_owned())
        })
    );
    let step = answered(&mut page, &db, step).await;
    assert_eq!(step, Step::default());
    assert_eq!(tools(&page), ["ops_delete"]);

    let step = page.update(McpLogMsg::Filter(SessionChoice(None)));
    answered(&mut page, &db, step).await;
    assert_eq!(tools(&page).len(), 3);
}

#[tokio::test]
async fn an_answer_for_another_filter_is_dropped() {
    let db = db().await;
    let mut page = McpLogPage::default();
    let stale = page.read().run(db.clone()).await;
    page.update(McpLogMsg::Filter(SessionChoice(Some("http:a".to_owned()))));
    page.update(stale);
    assert!(
        page.entries.is_none(),
        "the read of every session came late"
    );
    assert!(page.busy, "the read of http:a is still on its way");
}

#[tokio::test]
async fn refresh_reads_again_and_clear_empties_the_log() {
    let db = db().await;
    let mut page = McpLogPage::default();
    let call = page.read();
    page.update(call.run(db.clone()).await);
    db.record_mcp_call(&entry("http:c", "list_dir", "allowed"))
        .await
        .expect("record");
    let step = page.update(McpLogMsg::Refresh);
    answered(&mut page, &db, step).await;
    assert_eq!(tools(&page)[0], "list_dir");

    let step = page.update(McpLogMsg::Clear);
    assert_eq!(step.call, Some(McpLogCall::Clear));
    assert!(page.busy);
    assert_eq!(page.update(McpLogMsg::Clear), Step::default(), "once");
    let step = answered(&mut page, &db, step).await;
    let (note, error) = step.note.clone().expect("a note");
    assert!(note.contains('4'), "{note}");
    assert!(!error);
    answered(&mut page, &db, step).await;
    assert_eq!(tools(&page), Vec::<&str>::new());
    assert!(page.sessions.is_empty());
}

#[tokio::test]
async fn a_failed_read_leaves_a_note() {
    let mut page = McpLogPage::default();
    page.read();
    let step = page.update(McpLogMsg::Read(Err("database_error")));
    assert!(!page.busy);
    let (_, error) = step.note.expect("a note");
    assert!(error);
}

#[test]
fn a_line_names_the_decision_and_cuts_long_arguments() {
    let entry = rc_db::McpAuditEntry {
        id: 1,
        recorded_at: "2026-11-05T10:11:12.000Z".to_owned(),
        session: "http:a".to_owned(),
        tool: "ops_delete".to_owned(),
        class: Some("destructive".to_owned()),
        decision: "denied".to_owned(),
        code: "policy_denied".to_owned(),
        arguments: "x".repeat(500),
    };
    let line = entry_line(&entry);
    assert!(line.starts_with("2026-11-05 10:11"), "{line}");
    assert!(line.contains(decision_title("denied")), "{line}");
    assert!(line.contains("policy_denied"), "{line}");
    assert!(line.ends_with("..."), "{line}");
    assert!(line.len() < 300, "{line}");
    assert_ne!(decision_title("nonsense"), decision_title("allowed"));
}
