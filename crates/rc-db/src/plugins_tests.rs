use crate::{
    DURABLE_TABLES, Db, NewPluginExecution, RetentionLimits, RetentionSettings, retained_table,
};

const ID: &str = "019d0000-0000-7000-8000-0000000000a1";

async fn db() -> Db {
    Db::open_in_memory().await.expect("open")
}

fn call(operation: &str) -> NewPluginExecution {
    NewPluginExecution {
        plugin_id: ID.to_owned(),
        plugin_version: "1.0.0".to_owned(),
        plugin_type: "viewer".to_owned(),
        operation: operation.to_owned(),
        outcome: "ok".to_owned(),
        code: None,
        duration_ms: 3,
    }
}

#[tokio::test]
async fn a_plugin_row_keeps_what_the_person_decided_across_a_reinstall() {
    let db = db().await;
    let row = db.record_plugin(ID, false).await.expect("record");
    assert!(row.enabled && !row.bundled);
    assert!(db.set_plugin_enabled(ID, false).await.expect("switch"));
    assert!(
        db.set_plugin_versions(ID, Some("1.0.0"), None, Some("2.0.0"))
            .await
            .expect("versions")
    );
    let again = db.record_plugin(ID, true).await.expect("reinstall");
    assert!(!again.enabled, "still off");
    assert!(again.bundled);
    assert_eq!(again.active_version.as_deref(), Some("1.0.0"));
    assert_eq!(again.pending_version.as_deref(), Some("2.0.0"));
    assert_eq!(db.plugins().await.expect("read"), [again]);
    assert!(db.remove_plugin(ID).await.expect("remove"));
    assert_eq!(db.plugin(ID).await.expect("read"), None);
    assert!(!db.set_plugin_enabled(ID, true).await.expect("no row"));
}

#[tokio::test]
async fn a_confirmed_key_stays_pinned_and_can_be_revoked_once() {
    let db = db().await;
    let fingerprint = "ab".repeat(32);
    assert!(
        db.trust_plugin_key("author-v1", "AAAA", &fingerprint)
            .await
            .expect("trust")
    );
    assert!(
        !db.trust_plugin_key("author-v1", "BBBB", &"cd".repeat(32))
            .await
            .expect("pinned")
    );
    let keys = db.plugin_keys().await.expect("read");
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].public_key, "AAAA");
    assert!(db.revoke_plugin_key(&fingerprint).await.expect("revoke"));
    assert!(!db.revoke_plugin_key(&fingerprint).await.expect("revoked"));
    assert!(
        db.plugin_keys().await.expect("read")[0]
            .revoked_at
            .is_some()
    );
}

#[tokio::test]
async fn a_digest_is_withdrawn_once() {
    let db = db().await;
    let digest = "0f".repeat(32);
    assert!(
        db.revoke_plugin_digest(&digest, Some("broken"))
            .await
            .expect("withdraw")
    );
    assert!(!db.revoke_plugin_digest(&digest, None).await.expect("again"));
    assert_eq!(
        db.plugin_digest_revocations().await.expect("read"),
        [digest]
    );
}

/// The acceptance test of RC-100-09: `plugin_executions` is cut by the shared sweep.
#[tokio::test]
async fn the_execution_log_is_read_newest_first_and_cut_by_the_shared_sweep() {
    let db = db().await;
    let table = retained_table("plugin_executions").expect("retained");
    let rows = table.floor.max_rows + 20;
    for index in 0..rows {
        db.record_plugin_execution(&call(&format!("call{index}")))
            .await
            .expect("record");
    }
    let latest = db.plugin_executions(ID, 2).await.expect("read");
    assert_eq!(latest[0].operation, format!("call{}", rows - 1));
    assert_eq!(latest[0].outcome, "ok");

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
    assert_eq!(report.deleted, 20);
    let kept = db
        .plugin_executions(ID, crate::PLUGIN_EXECUTIONS_MAX_PAGE)
        .await
        .expect("read");
    assert_eq!(kept.len() as u64, table.floor.max_rows);
    assert_eq!(
        kept.last().map(|call| call.operation.as_str()),
        Some("call20")
    );
}

#[test]
fn the_plugin_tables_are_listed_as_kept() {
    for table in ["plugins", "plugin_keys", "plugin_digest_revocations"] {
        assert!(
            DURABLE_TABLES.iter().any(|(name, _)| *name == table),
            "{table}"
        );
    }
}
