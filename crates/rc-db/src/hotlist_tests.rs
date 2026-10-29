use super::*;

async fn db() -> Db {
    Db::open_in_memory().await.expect("open")
}

async fn entry(db: &Db, parent: Option<i64>, name: &str) -> HotlistItem {
    db.add_hotlist(&NewHotlistItem {
        parent_id: parent,
        name,
        location: Some(name),
        target: None,
    })
    .await
    .expect("add entry")
}

async fn folder(db: &Db, parent: Option<i64>, name: &str) -> HotlistItem {
    db.add_hotlist(&NewHotlistItem {
        parent_id: parent,
        name,
        location: None,
        target: Some("dropped"),
    })
    .await
    .expect("add folder")
}

fn names(items: &[HotlistItem]) -> Vec<&str> {
    items.iter().map(|item| item.name.as_str()).collect()
}

#[tokio::test]
async fn rows_are_added_in_order_and_listed_level_by_level() {
    let db = db().await;
    let projects = folder(&db, None, "Projects").await;
    entry(&db, None, "Home").await;
    entry(&db, Some(projects.id), "rCommander").await;
    let listed = db.hotlist().await.expect("list");
    assert_eq!(names(&listed), ["Projects", "Home", "rCommander"]);
    assert!(listed[0].is_folder());
    assert_eq!(listed[0].target, None, "a folder has no target");
    assert_eq!(listed[2].parent_id, Some(projects.id));
    assert_eq!((listed[0].sort_order, listed[1].sort_order), (0, 1));
}

#[tokio::test]
async fn names_are_trimmed_and_never_blank() {
    let db = db().await;
    assert_eq!(entry(&db, None, "  Docs ").await.name, "Docs");
    let blank = db
        .add_hotlist(&NewHotlistItem {
            parent_id: None,
            name: "  ",
            location: Some("x"),
            target: None,
        })
        .await;
    assert!(
        matches!(blank, Err(HotlistError::BadName { .. })),
        "{blank:?}"
    );
    let long = "x".repeat(HOTLIST_MAX_NAME + 1);
    let too_long = db
        .update_hotlist(
            1,
            &HotlistPatch {
                name: Some(long),
                ..HotlistPatch::default()
            },
        )
        .await;
    assert!(matches!(too_long, Err(HotlistError::BadName { .. })));
}

#[tokio::test]
async fn only_a_folder_holds_rows() {
    let db = db().await;
    let home = entry(&db, None, "Home").await;
    let inside = db
        .add_hotlist(&NewHotlistItem {
            parent_id: Some(home.id),
            name: "below",
            location: Some("x"),
            target: None,
        })
        .await;
    assert!(matches!(inside, Err(HotlistError::NotAFolder { id }) if id == home.id));
    let missing = db.move_hotlist(home.id, Some(999), None).await;
    assert!(matches!(missing, Err(HotlistError::NotFound { id: 999 })));
}

#[tokio::test]
async fn an_update_changes_what_it_names_and_keeps_the_rest() {
    let db = db().await;
    let home = db
        .add_hotlist(&NewHotlistItem {
            parent_id: None,
            name: "Home",
            location: Some("h"),
            target: Some("t"),
        })
        .await
        .expect("add");
    let renamed = db
        .update_hotlist(
            home.id,
            &HotlistPatch {
                name: Some("House".into()),
                ..HotlistPatch::default()
            },
        )
        .await
        .expect("rename");
    assert_eq!(
        (renamed.location.as_deref(), renamed.target.as_deref()),
        (Some("h"), Some("t"))
    );
    let untargeted = db
        .update_hotlist(
            home.id,
            &HotlistPatch {
                location: Some("h2".into()),
                target: Some(None),
                ..HotlistPatch::default()
            },
        )
        .await
        .expect("retarget");
    assert_eq!(untargeted.name, "House");
    assert_eq!(untargeted.location.as_deref(), Some("h2"));
    assert_eq!(untargeted.target, None);
    let projects = folder(&db, None, "Projects").await;
    let refused = db
        .update_hotlist(
            projects.id,
            &HotlistPatch {
                location: Some("x".into()),
                ..HotlistPatch::default()
            },
        )
        .await;
    assert!(matches!(refused, Err(HotlistError::NotAnEntry { .. })));
}

#[tokio::test]
async fn a_move_reorders_and_regroups() {
    let db = db().await;
    let a = entry(&db, None, "a").await;
    entry(&db, None, "b").await;
    let c = entry(&db, None, "c").await;
    db.move_hotlist(c.id, None, Some(0))
        .await
        .expect("to the front");
    assert_eq!(names(&db.hotlist().await.expect("list")), ["c", "a", "b"]);
    let group = folder(&db, None, "group").await;
    db.move_hotlist(a.id, Some(group.id), None)
        .await
        .expect("into the folder");
    let listed = db.hotlist().await.expect("list");
    assert_eq!(names(&listed), ["c", "b", "group", "a"]);
    assert_eq!(listed[3].parent_id, Some(group.id));
}

#[tokio::test]
async fn a_folder_never_moves_into_itself_or_below() {
    let db = db().await;
    let outer = folder(&db, None, "outer").await;
    let inner = folder(&db, Some(outer.id), "inner").await;
    for parent in [outer.id, inner.id] {
        let moved = db.move_hotlist(outer.id, Some(parent), None).await;
        assert!(matches!(moved, Err(HotlistError::Cycle)), "{moved:?}");
    }
}

#[tokio::test]
async fn nesting_stops_at_the_depth_limit() {
    let db = db().await;
    let mut levels = Vec::new();
    for level in 1..=HOTLIST_MAX_DEPTH {
        let parent = levels.last().copied();
        levels.push(folder(&db, parent, &format!("level{level}")).await.id);
    }
    let deeper = db
        .add_hotlist(&NewHotlistItem {
            parent_id: levels.last().copied(),
            name: "too deep",
            location: Some("x"),
            target: None,
        })
        .await;
    assert!(matches!(deeper, Err(HotlistError::TooDeep { .. })));
    // Two levels moved below level 7 would end at level 9.
    let top = folder(&db, None, "top").await;
    folder(&db, Some(top.id), "nested").await;
    let moved = db
        .move_hotlist(top.id, Some(levels[HOTLIST_MAX_DEPTH - 2]), None)
        .await;
    assert!(
        matches!(moved, Err(HotlistError::TooDeep { .. })),
        "{moved:?}"
    );
    db.move_hotlist(top.id, Some(levels[HOTLIST_MAX_DEPTH - 3]), None)
        .await
        .expect("level 7 and 8 fit");
}

#[tokio::test]
async fn a_folder_is_deleted_with_everything_in_it() {
    let db = db().await;
    let group = folder(&db, None, "group").await;
    let sub = folder(&db, Some(group.id), "sub").await;
    entry(&db, Some(sub.id), "deep").await;
    entry(&db, Some(group.id), "near").await;
    let kept = entry(&db, None, "kept").await;
    assert_eq!(db.delete_hotlist(group.id).await.expect("delete"), 4);
    assert_eq!(db.hotlist().await.expect("list"), vec![kept]);
    let gone = db.delete_hotlist(group.id).await;
    assert!(matches!(gone, Err(HotlistError::NotFound { .. })));
}
