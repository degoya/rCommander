//! What "allow for this session" offers for a call (RC-010-27), from the settings the gate
//! reads at once (RC-080-54), and the target an executing grant is bound to.

use rc_db::Db;
use rc_engine::{CommandClass, McpSessionOffer, settings::MCP_EXEC_SESSION_MINUTES_KEY};
use serde_json::{Map, Value, json};

use super::{exec_target, session_offer};
use crate::gate_settings::GateSettings;

/// What the gate would offer with the settings stored in `db` now.
async fn offer_of(
    db: &Db,
    class: CommandClass,
    tool: &str,
    arguments: Option<&Map<String, Value>>,
) -> Option<(McpSessionOffer, Option<String>)> {
    let settings = GateSettings::load(db).await.expect("settings");
    session_offer(&settings, class, tool, arguments)
}

#[tokio::test]
async fn what_allow_for_this_session_would_grant() {
    let db = Db::open_in_memory().await.expect("open");
    let terminal = json!({ "terminal": 3, "text": "ls" });
    let terminal = terminal.as_object();
    let connection = json!({ "connection_id": 7, "command": "uptime" });
    let connection = connection.as_object();
    let offer = |class, tool, arguments| offer_of(&db, class, tool, arguments);

    assert_eq!(
        offer(CommandClass::Exec, "terminal_send_input", terminal).await,
        Some((McpSessionOffer::Minutes(30), Some("terminal=3".to_owned())))
    );
    assert_eq!(
        offer(CommandClass::Exec, "ssh_exec", connection).await,
        Some((
            McpSessionOffer::Minutes(30),
            Some("connection_id=7".to_owned())
        ))
    );
    assert_eq!(offer(CommandClass::Exec, "terminal_open", None).await, None);
    assert_eq!(
        offer(CommandClass::Write, "ops_mkdir", None).await,
        Some((McpSessionOffer::Session, None))
    );
    assert_eq!(
        offer(CommandClass::Destructive, "ops_delete", None).await,
        None
    );

    db.set(MCP_EXEC_SESSION_MINUTES_KEY, &0)
        .await
        .expect("stores");
    assert_eq!(
        offer(CommandClass::Exec, "terminal_send_input", terminal).await,
        None,
        "0 minutes offers none"
    );
}

#[test]
fn an_exec_grant_is_bound_to_a_terminal_or_connection_by_number() {
    let arguments = |value: Value| value.as_object().cloned();
    let target = |tool, value| exec_target(tool, arguments(value).as_ref());
    assert_eq!(
        target("terminal_send_input", json!({ "terminal": 1 })),
        Some("terminal=1".to_owned())
    );
    assert_eq!(
        target("ssh_exec", json!({ "connection_id": 2 })),
        Some("connection_id=2".to_owned())
    );
    assert_eq!(
        target("terminal_send_input", json!({ "terminal": "1" })),
        None
    );
    assert_eq!(target("terminal_send_input", json!({ "text": "ls" })), None);
    assert_eq!(target("terminal_open", json!({ "terminal": 1 })), None);
    assert_eq!(exec_target("ssh_exec", None), None);
}
