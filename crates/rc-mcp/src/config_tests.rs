use rc_db::Db;
use rc_engine::settings::{is_protected, setting};

use super::*;

#[test]
fn the_keys_are_protected_schema_rows() {
    for key in [ENABLED_KEY, PORT_KEY] {
        assert!(setting(key).is_some_and(|spec| spec.protected), "{key}");
    }
    for key in [crate::TOKEN_KEY, crate::POLICY_KEY] {
        assert!(is_protected(key), "{key}");
    }
}

#[tokio::test]
async fn unset_means_on_with_any_port() {
    let db = Db::open_in_memory().await.expect("db");
    assert!(enabled(&db).await);
    assert_eq!(port(&db).await, 0);
}

#[tokio::test]
async fn stored_values_count_and_bad_ones_fall_back() {
    let db = Db::open_in_memory().await.expect("db");
    db.set(ENABLED_KEY, &false).await.expect("stored");
    db.set(PORT_KEY, &4711).await.expect("stored");
    assert!(!enabled(&db).await);
    assert_eq!(port(&db).await, 4711);

    db.set(ENABLED_KEY, "no").await.expect("stored");
    db.set(PORT_KEY, &70_000).await.expect("stored");
    assert!(enabled(&db).await, "a bad switch reads as its default");
    assert_eq!(port(&db).await, 0, "a port out of range reads as any");
}
