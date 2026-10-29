//! The grants of "allow for this session": bound to session, tool and target, run out on
//! time, capped, shown and revoked only through the window's command.

use std::time::{Duration, Instant};

use super::{Grants, MAX_MCP_GRANTS, McpGrant};
use crate::{Command, CommandClass, EngineEvent, Outcome, spec, test_support};

#[test]
fn a_grant_holds_for_its_session_tool_and_target_only() {
    let now = Instant::now();
    let mut grants = Grants::default();
    grants.grant("s1", "terminal_send_input", Some("terminal=1".into()), None);
    assert!(grants.holds("s1", "terminal_send_input", Some("terminal=1"), now));
    assert!(!grants.holds("s1", "terminal_send_input", Some("terminal=2"), now));
    assert!(!grants.holds("s1", "terminal_send_input", None, now));
    assert!(!grants.holds("s2", "terminal_send_input", Some("terminal=1"), now));
    assert!(!grants.holds("s1", "ssh_exec", Some("terminal=1"), now));
}

#[test]
fn a_grant_runs_out_at_its_time() {
    let now = Instant::now();
    let until = now + Duration::from_secs(1800);
    let mut grants = Grants::default();
    grants.grant(
        "s1",
        "ssh_exec",
        Some("connection_id=7".into()),
        Some(until),
    );
    let just_before = until - Duration::from_millis(1);
    assert!(grants.holds("s1", "ssh_exec", Some("connection_id=7"), just_before));
    assert!(!grants.holds("s1", "ssh_exec", Some("connection_id=7"), until));
    assert!(grants.live(until).is_empty(), "it went");
}

#[test]
fn the_oldest_grant_goes_first() {
    let now = Instant::now();
    let mut grants = Grants::default();
    for n in 0..=MAX_MCP_GRANTS {
        grants.grant("s1", "ops_mkdir", Some(n.to_string()), None);
    }
    assert_eq!(grants.live(now).len(), MAX_MCP_GRANTS);
    assert!(!grants.holds("s1", "ops_mkdir", Some("0"), now));
    assert!(grants.holds("s1", "ops_mkdir", Some("1"), now));
}

#[test]
fn only_the_window_revokes() {
    assert_eq!(
        spec("mcp.revoke").expect("a row").class,
        CommandClass::GuiOnly
    );
}

#[tokio::test]
async fn the_window_sees_and_revokes_the_grants() {
    let (_root, engine) = test_support::engine(&[]).await;
    let mut events = engine.subscribe();
    engine.allow_mcp_for_session("s1", "ops_mkdir", None, None);
    engine.allow_mcp_for_session(
        "s1",
        "terminal_send_input",
        Some("terminal=1".into()),
        Some(Duration::from_secs(1800)),
    );
    assert!(matches!(
        events.recv().await.expect("an event"),
        EngineEvent::McpGrantsChanged
    ));
    assert_eq!(
        engine.mcp_grants(),
        [
            McpGrant {
                tool: "ops_mkdir".into(),
                target: None
            },
            McpGrant {
                tool: "terminal_send_input".into(),
                target: Some("terminal=1".into())
            },
        ]
    );
    assert!(engine.mcp_allowed_for_session("s1", "terminal_send_input", Some("terminal=1")));

    let revoked = engine
        .dispatch(Command::RevokeMcpGrants {})
        .await
        .expect("revokes");
    assert_eq!(revoked, Outcome::McpRevoked { removed: 2 });
    assert!(engine.mcp_grants().is_empty());
    assert!(!engine.mcp_allowed_for_session("s1", "ops_mkdir", None));
}

#[tokio::test]
async fn a_grant_run_out_is_announced_when_pruned() {
    let (_root, engine) = test_support::engine(&[]).await;
    engine.allow_mcp_for_session(
        "s1",
        "ssh_exec",
        Some("connection_id=7".into()),
        Some(Duration::ZERO),
    );
    let mut events = engine.subscribe();
    engine.prune_mcp_grants();
    assert!(matches!(
        events.try_recv().expect("an event"),
        EngineEvent::McpGrantsChanged
    ));
    engine.prune_mcp_grants();
    assert!(events.try_recv().is_err(), "nothing went the second time");
}

/// The same grant given again is kept once, with the newer time (RC-080-50).
#[test]
fn a_grant_given_again_is_kept_once() {
    let now = Instant::now();
    let soon = now + Duration::from_secs(60);
    let mut grants = Grants::default();
    grants.grant("s1", "ssh_exec", Some("connection_id=7".into()), Some(soon));
    grants.grant("s1", "ssh_exec", Some("connection_id=7".into()), None);
    grants.grant("s2", "ssh_exec", Some("connection_id=7".into()), Some(soon));
    assert_eq!(grants.live(now).len(), 2, "one per session");
    assert!(grants.holds("s1", "ssh_exec", Some("connection_id=7"), soon));
    assert!(!grants.holds("s2", "ssh_exec", Some("connection_id=7"), soon));
}

/// A grant that ran out goes when the server asks about it or the window lists the grants, and
/// the window is told once (RC-080-50).
#[tokio::test]
async fn a_grant_run_out_is_announced_when_looked_at() {
    let (_root, engine) = test_support::engine(&[]).await;
    for look in ["asked", "listed"] {
        engine.allow_mcp_for_session(
            "s1",
            "ssh_exec",
            Some("connection_id=7".into()),
            Some(Duration::ZERO),
        );
        let mut events = engine.subscribe();
        match look {
            "asked" => {
                assert!(!engine.mcp_allowed_for_session("s1", "ssh_exec", Some("connection_id=7")))
            }
            _ => assert!(engine.mcp_grants().is_empty()),
        }
        assert!(
            matches!(events.try_recv(), Ok(EngineEvent::McpGrantsChanged)),
            "{look}"
        );
        assert!(engine.mcp_grants().is_empty());
        assert!(events.try_recv().is_err(), "{look}: told once");
    }
}
