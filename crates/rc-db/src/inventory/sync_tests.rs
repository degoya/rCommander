//! The acceptance's reconciliation (RC-100-39): a second sync without a change changes nothing;
//! a changed address updates and the person's own fields stay; a host that left is marked, not
//! deleted, and unmarked when it returns; an unread host is not taken for gone.

use super::{InventoryEntry, SyncStatus};
use crate::{
    Db,
    connections::{AuthMethod, ConnectionKind, ConnectionPatch, DefaultView},
    inventory::{InventorySourceDraft, InventorySourceId},
};

fn entry(id: &str, name: &str, address: &str) -> InventoryEntry {
    InventoryEntry {
        external_id: id.to_owned(),
        name: name.to_owned(),
        address: address.to_owned(),
        port: 22,
        kind: ConnectionKind::Ssh,
        default_view: DefaultView::Terminal,
        user: Some("deploy".to_owned()),
        folder: vec!["dc1".to_owned()],
        notes: Some("rack 4".to_owned()),
    }
}

async fn source(db: &Db) -> InventorySourceId {
    db.create_inventory_source(&InventorySourceDraft {
        name: "NetBox".to_owned(),
        plugin_id: "019d0000-0000-7000-8000-0000000000c1".to_owned(),
        address: "https://netbox.example.net/".to_owned(),
        folder_id: None,
    })
    .await
    .expect("a source")
}

fn ids(statuses: &[SyncStatus]) -> Vec<crate::connections::ConnectionId> {
    statuses
        .iter()
        .filter_map(|status| match status {
            SyncStatus::Created { id }
            | SyncStatus::Updated { id }
            | SyncStatus::Unchanged { id }
            | SyncStatus::Update { id } => Some(*id),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_preview_plans_and_writes_nothing() {
    let db = Db::open_in_memory().await.expect("open");
    let id = source(&db).await;
    let planned = db
        .sync_inventory(id, &[entry("1", "web-01", "web-01.lan")], &[], true)
        .await
        .expect("a preview");
    assert_eq!(planned.folder, None);
    assert_eq!(planned.statuses, [SyncStatus::New]);
    assert!(db.connection_tree().await.expect("tree").folders.is_empty());
    let stored = db.inventory_source(id).await.expect("the source");
    assert_eq!(stored.last_sync_at, None, "a preview is no sync");
}

#[tokio::test]
async fn a_second_sync_without_a_change_changes_nothing() {
    let db = Db::open_in_memory().await.expect("open");
    let id = source(&db).await;
    let entries = [
        entry("1", "web-01", "web-01.lan"),
        entry("2", "db-01", "10.0.0.5"),
    ];
    let first = db
        .sync_inventory(id, &entries, &[], false)
        .await
        .expect("a sync");
    assert_eq!(first.counts(), (2, 0, 0, 0));
    let created = ids(&first.statuses);
    let before = db.connection(created[0]).await.expect("a connection");
    assert_eq!(before.default_view, DefaultView::Terminal);
    assert_eq!(before.username.as_deref(), Some("deploy"));

    let second = db
        .sync_inventory(id, &entries, &[], false)
        .await
        .expect("a sync");
    assert_eq!(second.counts(), (0, 0, 2, 0));
    assert!(second.missing.is_empty());
    let after = db.connection(created[0]).await.expect("a connection");
    assert_eq!(after.updated_at, before.updated_at, "nothing was written");
    let stored = db.inventory_source(id).await.expect("the source");
    assert_eq!(stored.last_outcome.as_deref(), Some("ok"));
    assert_eq!(stored.folder_id, first.folder);
}

#[tokio::test]
async fn a_changed_address_updates_and_own_fields_stay() {
    let db = Db::open_in_memory().await.expect("open");
    let id = source(&db).await;
    let first = db
        .sync_inventory(id, &[entry("1", "web-01", "web-01.lan")], &[], false)
        .await
        .expect("a sync");
    let connection = ids(&first.statuses)[0];
    // The person's own fields: user, auth method, notes.
    db.update_connection(
        connection,
        &ConnectionPatch {
            username: Some("root".to_owned()),
            auth_method: Some(AuthMethod::Agent),
            notes: Some("mine".to_owned()),
            ..ConnectionPatch::default()
        },
    )
    .await
    .expect("the person's change");

    let mut moved = entry("1", "web-01-new", "10.1.1.1");
    moved.port = 2222;
    moved.folder = vec!["dc2".to_owned(), "web".to_owned()];
    moved.user = Some("inventory-user".to_owned());
    let second = db
        .sync_inventory(id, &[moved], &[], false)
        .await
        .expect("a sync");
    assert_eq!(second.statuses, [SyncStatus::Updated { id: connection }]);
    let updated = db.connection(connection).await.expect("a connection");
    assert_eq!(updated.name, "web-01-new");
    assert_eq!(updated.host, "10.1.1.1");
    assert_eq!(updated.port, 2222);
    assert_ne!(updated.folder_id, first.folder);
    assert_eq!(updated.username.as_deref(), Some("root"));
    assert_eq!(updated.auth_method, AuthMethod::Agent);
    assert_eq!(updated.notes.as_deref(), Some("mine"));
}

#[tokio::test]
async fn a_host_that_left_is_marked_and_unmarked_when_it_returns() {
    let db = Db::open_in_memory().await.expect("open");
    let id = source(&db).await;
    let both = [
        entry("1", "web-01", "web-01.lan"),
        entry("2", "db-01", "10.0.0.5"),
    ];
    let first = db
        .sync_inventory(id, &both, &[], false)
        .await
        .expect("a sync");
    let gone = ids(&first.statuses)[1];

    let left = db
        .sync_inventory(id, &both[..1], &[], false)
        .await
        .expect("a sync");
    assert_eq!(left.missing, [gone]);
    db.connection(gone).await.expect("never deleted");
    let links = db.inventory_links().await.expect("links");
    let link = links
        .iter()
        .find(|link| link.connection == gone)
        .expect("a link");
    assert!(link.missing_since.is_some());

    let again = db
        .sync_inventory(id, &both[..1], &[], false)
        .await
        .expect("a sync");
    assert!(again.missing.is_empty());
    assert_eq!(again.still_missing, 1);

    let back = db
        .sync_inventory(id, &both, &[], false)
        .await
        .expect("a sync");
    assert_eq!(back.returned, [gone]);
    let links = db.inventory_links().await.expect("links");
    assert!(links.iter().all(|link| link.missing_since.is_none()));
}

/// A host the plugin listed but the plugin host could not read is not gone.
#[tokio::test]
async fn an_unread_host_is_not_marked() {
    let db = Db::open_in_memory().await.expect("open");
    let id = source(&db).await;
    db.sync_inventory(id, &[entry("1", "web-01", "web-01.lan")], &[], false)
        .await
        .expect("a sync");
    let unread = db
        .sync_inventory(id, &[], &["1".to_owned()], false)
        .await
        .expect("a sync");
    assert!(unread.missing.is_empty());
}

#[tokio::test]
async fn a_name_taken_or_a_bad_host_is_refused_alone() {
    let db = Db::open_in_memory().await.expect("open");
    let id = source(&db).await;
    let mut bad = entry("3", "bad", "has space");
    bad.port = 22;
    let done = db
        .sync_inventory(
            id,
            &[
                entry("1", "web-01", "web-01.lan"),
                entry("2", "WEB-01", "other.lan"),
                bad,
                entry("1", "dup", "dup.lan"),
            ],
            &[],
            false,
        )
        .await
        .expect("a sync");
    assert!(matches!(done.statuses[0], SyncStatus::Created { .. }));
    assert_eq!(
        &done.statuses[1..],
        [
            SyncStatus::Refused {
                code: "name_taken".to_owned()
            },
            SyncStatus::Refused {
                code: "host_invalid".to_owned()
            },
            SyncStatus::Refused {
                code: "duplicate".to_owned()
            },
        ]
    );
}

/// Deleting a source keeps its connections as ordinary ones.
#[tokio::test]
async fn a_deleted_source_leaves_its_connections() {
    let db = Db::open_in_memory().await.expect("open");
    let id = source(&db).await;
    let done = db
        .sync_inventory(id, &[entry("1", "web-01", "web-01.lan")], &[], false)
        .await
        .expect("a sync");
    db.delete_inventory_source(id).await.expect("deleted");
    db.connection(ids(&done.statuses)[0])
        .await
        .expect("the connection stays");
    assert!(db.inventory_links().await.expect("links").is_empty());
}
