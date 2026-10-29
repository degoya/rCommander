use rc_db::{Db, DbError};
use rc_engine::settings::{MCP_EXEC_SESSION_MINUTES_KEY, MCP_HEADLESS_CODE_KEY};

use super::GateSettings;
use crate::policy::{POLICY_KEY, Policy, Rule};

#[tokio::test]
async fn nothing_stored_reads_as_the_defaults() {
    let db = Db::open_in_memory().await.expect("open");
    let settings = GateSettings::load(&db).await.expect("loads");
    assert_eq!(
        settings,
        GateSettings {
            policy: Policy::default(),
            headless_codes: false,
            exec_minutes: 30,
        }
    );
}

#[tokio::test]
async fn the_three_stored_settings_are_read_at_once() {
    let db = Db::open_in_memory().await.expect("open");
    let policy = Policy {
        read: Rule::Allowed,
        write: Rule::Ask,
        destructive: Rule::Deny,
        exec: Rule::Ask,
    };
    policy.store(&db).await.expect("stores");
    db.set(MCP_HEADLESS_CODE_KEY, &true).await.expect("stores");
    db.set(MCP_EXEC_SESSION_MINUTES_KEY, &5)
        .await
        .expect("stores");
    assert_eq!(
        GateSettings::load(&db).await.expect("loads"),
        GateSettings {
            policy,
            headless_codes: true,
            exec_minutes: 5,
        }
    );
}

/// As each read alone: a bad switch or number is its default, a bad policy refuses.
#[tokio::test]
async fn a_bad_value_spoils_only_its_own_setting() {
    let db = Db::open_in_memory().await.expect("open");
    db.set(MCP_HEADLESS_CODE_KEY, "yes").await.expect("stores");
    db.set(MCP_EXEC_SESSION_MINUTES_KEY, "soon")
        .await
        .expect("stores");
    let settings = GateSettings::load(&db).await.expect("loads");
    assert!(!settings.headless_codes);
    assert_eq!(settings.exec_minutes, 30);

    db.set(POLICY_KEY, "open").await.expect("stores");
    assert!(matches!(
        GateSettings::load(&db).await,
        Err(DbError::Json { .. })
    ));
}
