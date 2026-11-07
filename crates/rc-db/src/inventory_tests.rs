use crate::{
    Db, SECRET_COLUMNS,
    inventory::{InventoryError, InventorySourceDraft},
};

fn draft(name: &str, address: &str) -> InventorySourceDraft {
    InventorySourceDraft {
        name: name.to_owned(),
        plugin_id: "019d0000-0000-7000-8000-0000000000c1".to_owned(),
        address: address.to_owned(),
        folder_id: None,
    }
}

#[tokio::test]
async fn a_source_is_kept_with_the_slots_it_holds() {
    let db = Db::open_in_memory().await.expect("open");
    let id = db
        .create_inventory_source(&draft(" NetBox ", "https://netbox.example.net/"))
        .await
        .expect("a source");
    let stored = db
        .store_inventory_secret(id, "api_token", |row| {
            Ok::<_, ()>(format!("sealed for {row}").into_bytes())
        })
        .await
        .expect("stored");
    assert!(stored.is_ok());
    let sources = db.inventory_sources().await.expect("sources");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].name, "NetBox");
    assert_eq!(sources[0].slots, ["api_token"]);
    let (row, cipher) = db
        .inventory_secret(id, "api_token")
        .await
        .expect("read")
        .expect("a value");
    assert_eq!(cipher, format!("sealed for {row}").into_bytes());
    assert_eq!(db.stored_secrets(SECRET_COLUMNS).await.expect("count"), 1);
}

#[tokio::test]
async fn bad_fields_are_refused_with_their_code() {
    let db = Db::open_in_memory().await.expect("open");
    for (bad, code) in [
        (draft("", "https://x/"), "name_invalid"),
        (draft("a\nb", "https://x/"), "name_invalid"),
        (draft("NetBox", ""), "inventory_address_invalid"),
        (draft("NetBox", "https://x/ y"), "inventory_address_invalid"),
        (
            InventorySourceDraft {
                plugin_id: "../../etc".to_owned(),
                ..draft("NetBox", "https://x/")
            },
            "inventory_plugin_invalid",
        ),
    ] {
        let error = db.create_inventory_source(&bad).await.expect_err("refused");
        assert_eq!(error.code(), code, "{bad:?}");
    }
    db.create_inventory_source(&draft("NetBox", "https://x/"))
        .await
        .expect("a source");
    let error = db
        .create_inventory_source(&draft("netbox", "https://y/"))
        .await
        .expect_err("taken");
    assert!(matches!(error, InventoryError::NameTaken { .. }));
}

/// A token was given for one address: a new address drops it.
#[tokio::test]
async fn a_new_address_drops_the_stored_secrets() {
    let db = Db::open_in_memory().await.expect("open");
    let id = db
        .create_inventory_source(&draft("NetBox", "https://netbox.example.net/"))
        .await
        .expect("a source");
    db.store_inventory_secret(id, "api_token", |_| Ok::<_, ()>(vec![1, 2, 3]))
        .await
        .expect("stored")
        .expect("sealed");
    db.update_inventory_source(id, &draft("NetBox", "https://netbox.example.net/"))
        .await
        .expect("unchanged address");
    assert!(
        db.inventory_secret(id, "api_token")
            .await
            .expect("read")
            .is_some()
    );
    db.update_inventory_source(id, &draft("NetBox", "https://evil.example.net/"))
        .await
        .expect("a new address");
    assert!(
        db.inventory_secret(id, "api_token")
            .await
            .expect("read")
            .is_none()
    );
}

#[tokio::test]
async fn a_slot_is_a_plain_name() {
    let db = Db::open_in_memory().await.expect("open");
    let id = db
        .create_inventory_source(&draft("NetBox", "https://x/"))
        .await
        .expect("a source");
    let refused = db
        .store_inventory_secret(id, "API Token", |_| Ok::<_, ()>(vec![1]))
        .await;
    assert!(matches!(refused, Err(InventoryError::SlotInvalid)));
}
