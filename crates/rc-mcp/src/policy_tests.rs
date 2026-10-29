use rc_db::Db;
use rc_engine::CommandClass;

use super::{POLICY_KEY, Policy, Rule};

#[test]
fn the_default_allows_reading_and_writing_and_asks_for_the_rest() {
    let policy = Policy::default();
    assert_eq!(policy.decide(CommandClass::Read), Rule::Allowed);
    assert_eq!(policy.decide(CommandClass::Write), Rule::Allowed);
    assert_eq!(policy.decide(CommandClass::Destructive), Rule::Ask);
    assert_eq!(policy.decide(CommandClass::Exec), Rule::Ask);
}

#[test]
fn destructive_and_exec_never_run_without_a_code() {
    let policy = Policy {
        read: Rule::Deny,
        write: Rule::Ask,
        destructive: Rule::Allowed,
        exec: Rule::Deny,
    };
    assert_eq!(policy.decide(CommandClass::Read), Rule::Deny);
    assert_eq!(policy.decide(CommandClass::Write), Rule::Ask);
    assert_eq!(policy.decide(CommandClass::Destructive), Rule::Ask);
    assert_eq!(policy.decide(CommandClass::Exec), Rule::Deny);
}

#[test]
fn gui_only_is_denied_whatever_the_setting() {
    let open = Policy {
        read: Rule::Allowed,
        write: Rule::Allowed,
        destructive: Rule::Allowed,
        exec: Rule::Allowed,
    };
    assert_eq!(open.decide(CommandClass::GuiOnly), Rule::Deny);
    assert_eq!(Policy::default().decide(CommandClass::GuiOnly), Rule::Deny);
}

#[tokio::test]
async fn the_policy_round_trips_through_the_settings_and_fills_gaps() {
    let db = Db::open_in_memory().await.expect("db");
    assert_eq!(Policy::load(&db).await.expect("loads"), Policy::default());
    db.set(POLICY_KEY, &serde_json::json!({ "write": "deny" }))
        .await
        .expect("stores");
    let partial = Policy::load(&db).await.expect("loads");
    assert_eq!(partial.write, Rule::Deny);
    assert_eq!(partial.destructive, Rule::Ask);
    let changed = Policy {
        exec: Rule::Deny,
        ..partial
    };
    changed.store(&db).await.expect("stores");
    assert_eq!(Policy::load(&db).await.expect("loads"), changed);
}
