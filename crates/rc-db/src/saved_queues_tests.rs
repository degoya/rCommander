use crate::{DURABLE_TABLES, Db};

async fn db() -> Db {
    Db::open_in_memory().await.expect("open")
}

fn specs(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|text| (*text).to_owned()).collect()
}

#[tokio::test]
async fn a_list_is_saved_read_back_in_order_and_deleted() {
    let db = db().await;
    assert!(db.saved_queues().await.expect("read").is_empty());
    let saved = db
        .save_queue(
            "Backup NAS",
            true,
            &specs(&["{\"n\":1}", "{\"n\":2}", "{\"n\":3}"]),
        )
        .await
        .expect("save");
    assert_eq!((saved.name.as_str(), saved.entries), ("Backup NAS", 3));
    assert!(saved.restore_on_start);
    assert!(saved.saved_at.ends_with('Z'));
    let other = db
        .save_queue("Albums", false, &specs(&["{}"]))
        .await
        .expect("save");
    let names: Vec<String> = db
        .saved_queues()
        .await
        .expect("read")
        .into_iter()
        .map(|queue| queue.name)
        .collect();
    assert_eq!(names, ["Albums", "Backup NAS"], "by name");

    let (read, entries) = db
        .saved_queue(saved.id)
        .await
        .expect("read")
        .expect("there");
    assert_eq!(read, saved);
    assert_eq!(entries, ["{\"n\":1}", "{\"n\":2}", "{\"n\":3}"]);

    assert!(db.delete_saved_queue(saved.id).await.expect("delete"));
    assert!(!db.delete_saved_queue(saved.id).await.expect("gone already"));
    assert_eq!(db.saved_queue(saved.id).await.expect("read"), None);
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM saved_queue_entries")
        .fetch_one(db.readers())
        .await
        .expect("count");
    assert_eq!(left, 1, "the entries go with their list: {other:?}");
}

#[tokio::test]
async fn saving_under_a_name_again_replaces_that_list() {
    let db = db().await;
    let first = db
        .save_queue("Platte G", false, &specs(&["{\"a\":1}", "{\"a\":2}"]))
        .await
        .expect("save");
    let again = db
        .save_queue("platte g", true, &specs(&["{\"b\":1}"]))
        .await
        .expect("save");
    assert_eq!(again.id, first.id, "case does not count");
    assert_eq!((again.name.as_str(), again.entries), ("platte g", 1));
    assert!(again.restore_on_start);
    let (_, entries) = db
        .saved_queue(first.id)
        .await
        .expect("read")
        .expect("there");
    assert_eq!(entries, ["{\"b\":1}"]);
    assert_eq!(db.saved_queues().await.expect("read").len(), 1);
}

#[tokio::test]
async fn the_migration_refuses_what_is_not_json_or_a_blank_name() {
    let db = db().await;
    let kept = db
        .save_queue("Kept", false, &specs(&["{\"n\":1}"]))
        .await
        .expect("save");
    assert!(
        db.save_queue("Kept", false, &specs(&["{broken"]))
            .await
            .is_err()
    );
    let (_, entries) = db.saved_queue(kept.id).await.expect("read").expect("there");
    assert_eq!(entries, ["{\"n\":1}"], "a refused save changes nothing");
    assert!(db.save_queue("   ", false, &[]).await.is_err());
}

#[test]
fn the_saved_lists_are_durable() {
    for table in ["saved_queues", "saved_queue_entries"] {
        assert!(
            DURABLE_TABLES.iter().any(|(name, _)| *name == table),
            "{table}"
        );
    }
}
