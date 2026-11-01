use crate::{DURABLE_TABLES, Db, WorkspacePatch};

async fn db() -> Db {
    Db::open_in_memory().await.expect("open")
}

const SPEC: &str = "{\"version\":1}";

async fn names(db: &Db) -> Vec<String> {
    db.workspaces()
        .await
        .expect("read")
        .into_iter()
        .map(|workspace| workspace.name)
        .collect()
}

#[test]
fn the_table_is_durable() {
    assert!(
        DURABLE_TABLES
            .iter()
            .any(|(table, _)| *table == "workspaces"),
        "a person makes and removes workspaces"
    );
}

#[tokio::test]
async fn a_workspace_is_created_read_changed_and_deleted() {
    let db = db().await;
    assert!(db.workspaces().await.expect("read").is_empty());
    let made = db
        .create_workspace("Servers", SPEC, Some("nord-light"), None)
        .await
        .expect("create");
    assert_eq!(made.name, "Servers");
    assert_eq!(made.sort_order, 0);
    assert_eq!(made.theme_light.as_deref(), Some("nord-light"));
    assert_eq!(made.theme_dark, None);
    assert!(made.created_at.ends_with('Z'));
    assert_eq!(
        db.workspace(made.id).await.expect("read"),
        Some(made.clone())
    );

    let patch = WorkspacePatch {
        name: Some("Web servers".into()),
        spec: Some("{\"version\":1,\"left\":{}}".into()),
        theme_light: Some(None),
        theme_dark: Some(Some("nord-dark".into())),
    };
    let changed = db
        .update_workspace(made.id, &patch)
        .await
        .expect("update")
        .expect("there");
    assert_eq!(changed.name, "Web servers");
    assert_eq!(changed.spec, "{\"version\":1,\"left\":{}}");
    assert_eq!(changed.theme_light, None, "Some(None) clears");
    assert_eq!(changed.theme_dark.as_deref(), Some("nord-dark"));

    let untouched = db
        .update_workspace(made.id, &WorkspacePatch::default())
        .await
        .expect("update")
        .expect("there");
    assert_eq!(
        untouched.name, "Web servers",
        "an empty patch changes nothing"
    );
    assert_eq!(untouched.theme_dark.as_deref(), Some("nord-dark"));

    assert!(db.delete_workspace(made.id).await.expect("delete"));
    assert!(!db.delete_workspace(made.id).await.expect("gone already"));
    assert_eq!(db.workspace(made.id).await.expect("read"), None);
    assert_eq!(
        db.update_workspace(made.id, &patch).await.expect("update"),
        None
    );
}

#[tokio::test]
async fn a_name_is_unique_without_regard_to_case_and_never_blank() {
    let db = db().await;
    db.create_workspace("Servers", SPEC, None, None)
        .await
        .expect("create");
    let taken = db
        .create_workspace("SERVERS", SPEC, None, None)
        .await
        .expect_err("taken");
    assert!(taken.is_unique_violation(), "{taken}");
    let blank = db
        .create_workspace("   ", SPEC, None, None)
        .await
        .expect_err("blank");
    assert!(!blank.is_unique_violation(), "{blank}");
    let other = db
        .create_workspace("Local", SPEC, None, None)
        .await
        .expect("create");
    let renamed = db
        .update_workspace(
            other.id,
            &WorkspacePatch {
                name: Some("servers".into()),
                ..WorkspacePatch::default()
            },
        )
        .await
        .expect_err("taken");
    assert!(renamed.is_unique_violation(), "{renamed}");
    let not_json = db
        .create_workspace("Broken", "{", None, None)
        .await
        .expect_err("not JSON");
    assert!(!not_json.is_unique_violation());
    assert_eq!(names(&db).await, ["Servers", "Local"]);
}

#[tokio::test]
async fn new_ones_go_last_and_a_move_numbers_the_list_anew() {
    let db = db().await;
    let mut ids = Vec::new();
    for name in ["A", "B", "C", "D"] {
        let made = db
            .create_workspace(name, SPEC, None, None)
            .await
            .expect("create");
        ids.push(made.id);
    }
    assert_eq!(names(&db).await, ["A", "B", "C", "D"]);
    assert!(db.move_workspace(ids[3], 0).await.expect("move"));
    assert_eq!(names(&db).await, ["D", "A", "B", "C"]);
    assert!(db.move_workspace(ids[3], 99).await.expect("move"));
    assert_eq!(
        names(&db).await,
        ["A", "B", "C", "D"],
        "beyond the end: last"
    );
    assert!(db.move_workspace(ids[0], 2).await.expect("move"));
    assert_eq!(names(&db).await, ["B", "C", "A", "D"]);
    let orders: Vec<i64> = db
        .workspaces()
        .await
        .expect("read")
        .into_iter()
        .map(|workspace| workspace.sort_order)
        .collect();
    assert_eq!(orders, [0, 1, 2, 3]);
    assert!(!db.move_workspace(999, 0).await.expect("no such one"));
    let after = db
        .create_workspace("E", SPEC, None, None)
        .await
        .expect("create");
    assert_eq!(after.sort_order, 4);
}
