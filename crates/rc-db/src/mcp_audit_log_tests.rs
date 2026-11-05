use crate::{
    Db, MCP_AUDIT_ARGUMENTS_MAX_BYTES, MCP_AUDIT_MAX_PAGE, MCP_AUDIT_NAME_MAX_BYTES,
    NewMcpAuditEntry, RetentionLimits, RetentionSettings, retained_table,
};

use super::cut_to_bytes;

async fn db() -> Db {
    Db::open_in_memory().await.expect("open")
}

fn entry(session: &str, tool: &str) -> NewMcpAuditEntry {
    NewMcpAuditEntry {
        session: session.to_owned(),
        tool: tool.to_owned(),
        class: Some("read".to_owned()),
        decision: "allowed".to_owned(),
        code: "ok".to_owned(),
        arguments: r#"{"panel":"left"}"#.to_owned(),
    }
}

#[tokio::test]
async fn calls_are_read_newest_first_by_session_and_cleared() {
    let db = db().await;
    db.record_mcp_call(&entry("http:a", "navigate"))
        .await
        .expect("record");
    db.record_mcp_call(&entry("http:b", "panel_state"))
        .await
        .expect("record");
    db.record_mcp_call(&entry("http:a", "files_read"))
        .await
        .expect("record");

    let all = db.mcp_audit_log(None, 10).await.expect("read");
    let tools: Vec<&str> = all.iter().map(|row| row.tool.as_str()).collect();
    assert_eq!(tools, ["files_read", "panel_state", "navigate"]);
    assert_eq!(all[0].class.as_deref(), Some("read"));
    assert_eq!(all[0].decision, "allowed");
    assert_eq!(all[0].code, "ok");
    assert_eq!(all[0].arguments, r#"{"panel":"left"}"#);

    let of_a = db.mcp_audit_log(Some("http:a"), 10).await.expect("read");
    assert_eq!(of_a.len(), 2);
    assert!(of_a.iter().all(|row| row.session == "http:a"));
    assert_eq!(
        db.mcp_audit_sessions().await.expect("sessions"),
        ["http:a", "http:b"]
    );
    assert_eq!(db.mcp_audit_log(None, 1).await.expect("read").len(), 1);

    assert_eq!(db.clear_mcp_audit_log().await.expect("clear"), 3);
    assert!(db.mcp_audit_log(None, 10).await.expect("read").is_empty());
    assert!(db.mcp_audit_sessions().await.expect("sessions").is_empty());
}

/// An agent names the tool and fills the arguments: neither writes past its bound.
#[tokio::test]
async fn every_text_is_cut_to_its_bound() {
    let db = db().await;
    let mut long = entry(&"s".repeat(500), &"t".repeat(500));
    long.arguments = "\u{e9}".repeat(MCP_AUDIT_ARGUMENTS_MAX_BYTES);
    long.decision = "d".repeat(100);
    db.record_mcp_call(&long).await.expect("record");
    let row = db
        .mcp_audit_log(None, MCP_AUDIT_MAX_PAGE)
        .await
        .expect("read")
        .remove(0);
    assert_eq!(row.session.len(), MCP_AUDIT_NAME_MAX_BYTES);
    assert_eq!(row.tool.len(), MCP_AUDIT_NAME_MAX_BYTES);
    assert_eq!(row.arguments.len(), MCP_AUDIT_ARGUMENTS_MAX_BYTES);
    assert_eq!(row.decision.len(), 32);
}

#[test]
fn a_cut_keeps_whole_characters() {
    assert_eq!(cut_to_bytes("abc", 5), "abc");
    assert_eq!(cut_to_bytes("abc", 2), "ab");
    // U+00E9 is two bytes: three bytes keep one of them.
    assert_eq!(cut_to_bytes("\u{e9}\u{e9}", 3), "\u{e9}");
    assert_eq!(cut_to_bytes("\u{e9}", 1), "");
}

/// The acceptance test of RC-080-66: the log is cut by the shared sweep.
#[tokio::test]
async fn the_log_is_cut_by_the_shared_sweep() {
    let db = db().await;
    let table = retained_table("mcp_audit_log").expect("retained");
    assert_eq!(
        table.defaults,
        RetentionLimits {
            max_rows: 10_000,
            max_age_days: 30
        }
    );
    let rows = table.floor.max_rows + 15;
    for index in 0..rows {
        db.record_mcp_call(&entry("http:a", &format!("tool{index}")))
            .await
            .expect("record");
    }
    let checked = table
        .check(RetentionLimits {
            max_rows: table.floor.max_rows,
            max_age_days: table.defaults.max_age_days,
        })
        .expect("in range");
    let report = db
        .prune(&RetentionSettings::defaults().with(checked))
        .await
        .expect("prune");
    assert_eq!(report.deleted, 15);
    let kept = db
        .mcp_audit_log(None, MCP_AUDIT_MAX_PAGE)
        .await
        .expect("read");
    assert_eq!(kept.len() as u64, table.floor.max_rows);
    assert_eq!(kept.last().map(|row| row.tool.as_str()), Some("tool15"));
}
