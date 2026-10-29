use tempfile::TempDir;

use super::*;
use crate::test_support;
use crate::{
    Command, PanelId,
    tabs::requests::{
        AddHotlistFolderRequest, AddHotlistRequest, MoveHotlistRequest, OpenHotlistRequest,
        UpdateHotlistRequest,
    },
};

async fn engine() -> (TempDir, Engine) {
    test_support::engine_with_db(&[("work/rc/", ""), ("docs/", "")]).await
}

async fn run(engine: &Engine, command: Command) -> Outcome {
    engine.dispatch(command).await.expect("command")
}

fn item(outcome: Outcome) -> HotlistEntry {
    match outcome {
        Outcome::HotlistItem { item } => item,
        other => panic!("a hotlist item, got {other:?}"),
    }
}

async fn listed(engine: &Engine) -> Vec<HotlistEntry> {
    match run(engine, Command::ListHotlist {}).await {
        Outcome::Hotlist { items } => items,
        other => panic!("the hotlist, got {other:?}"),
    }
}

fn add(path: &str) -> Command {
    Command::AddHotlist(AddHotlistRequest {
        path: Some(path.into()),
        ..AddHotlistRequest::default()
    })
}

/// The acceptance check of RC-010-03: Ctrl+D adds the panel's folder; entries are edited and
/// grouped in folders, which list before what they hold.
#[tokio::test]
async fn the_hotlist_is_filled_edited_and_grouped() {
    let (root, engine) = engine().await;
    let here = item(run(&engine, Command::AddHotlist(AddHotlistRequest::default())).await);
    assert_eq!(here.path, Some(VfsPath::local(root.path())));
    assert!(!here.folder);
    let rc = item(run(&engine, add("work/rc")).await);
    assert_eq!(rc.name, "rc", "named after the folder");
    let group = item(
        run(
            &engine,
            Command::AddHotlistFolder(AddHotlistFolderRequest {
                name: "Work".into(),
                parent_id: None,
            }),
        )
        .await,
    );
    assert!(group.folder && group.path.is_none());
    run(
        &engine,
        Command::MoveHotlist(MoveHotlistRequest {
            id: rc.id,
            parent_id: Some(group.id),
            position: None,
        }),
    )
    .await;
    let renamed = item(
        run(
            &engine,
            Command::UpdateHotlist(UpdateHotlistRequest {
                id: rc.id,
                panel: None,
                name: Some("rCommander".into()),
                path: None,
                target: Some("docs".into()),
                clear_target: false,
            }),
        )
        .await,
    );
    assert_eq!(
        renamed.target,
        Some(VfsPath::local(root.path().join("docs")))
    );
    let names: Vec<String> = listed(&engine)
        .await
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    let top = here.name.clone();
    assert_eq!(names, [top.as_str(), "Work", "rCommander"]);
    let deleted = run(&engine, Command::DeleteHotlist { id: group.id }).await;
    assert_eq!(
        deleted,
        Outcome::HotlistDeleted {
            id: group.id,
            removed: 2
        }
    );
    assert_eq!(listed(&engine).await.len(), 1);
}

#[tokio::test]
async fn opening_an_entry_shows_its_folder_and_its_target() {
    let (root, engine) = engine().await;
    let entry = item(
        run(
            &engine,
            Command::AddHotlist(AddHotlistRequest {
                path: Some("work/rc".into()),
                target: Some("docs".into()),
                ..AddHotlistRequest::default()
            }),
        )
        .await,
    );
    run(
        &engine,
        Command::OpenHotlist(OpenHotlistRequest {
            id: entry.id,
            panel: Some(PanelId::Right),
        }),
    )
    .await;
    assert_eq!(
        engine.panel(PanelId::Right).path,
        VfsPath::local(root.path().join("work").join("rc"))
    );
    assert_eq!(
        engine.panel(PanelId::Left).path,
        VfsPath::local(root.path().join("docs"))
    );
}

#[tokio::test]
async fn a_hotlist_folder_opens_nothing_and_refusals_have_codes() {
    let (_root, engine) = engine().await;
    let group = item(
        run(
            &engine,
            Command::AddHotlistFolder(AddHotlistFolderRequest {
                name: "Group".into(),
                parent_id: None,
            }),
        )
        .await,
    );
    let code = |command| {
        let engine = engine.clone();
        async move { engine.dispatch(command).await.expect_err("refused").code() }
    };
    assert_eq!(
        code(Command::OpenHotlist(OpenHotlistRequest {
            id: group.id,
            panel: None
        }))
        .await,
        "hotlist_not_an_entry"
    );
    assert_eq!(
        code(Command::MoveHotlist(MoveHotlistRequest {
            id: group.id,
            parent_id: Some(group.id),
            position: None
        }))
        .await,
        "hotlist_cycle"
    );
    assert_eq!(
        code(Command::DeleteHotlist { id: 999 }).await,
        "hotlist_not_found"
    );
    assert_eq!(
        code(Command::AddHotlistFolder(AddHotlistFolderRequest {
            name: " ".into(),
            parent_id: None
        }))
        .await,
        "hotlist_bad_name"
    );
}

#[tokio::test]
async fn a_change_is_announced() {
    let (_root, engine) = engine().await;
    let mut events = engine.subscribe();
    run(&engine, add("docs")).await;
    assert_eq!(events.try_recv().ok(), Some(EngineEvent::HotlistChanged));
}

#[test]
fn the_menu_order_puts_each_folder_before_what_it_holds() {
    let row = |id, parent_id, location: Option<&str>| HotlistItem {
        id,
        parent_id,
        name: format!("n{id}"),
        location: location.map(str::to_owned),
        target: None,
        sort_order: 0,
    };
    // As rc-db lists them: the top level first, then each folder's rows.
    let ordered = menu_order(vec![
        row(1, None, None),
        row(2, None, Some("null")),
        row(3, Some(1), None),
        row(4, Some(3), Some("null")),
        row(5, Some(1), Some("null")),
    ]);
    let ids: Vec<i64> = ordered.iter().map(|entry| entry.id).collect();
    assert_eq!(ids, [1, 3, 4, 5, 2]);
    assert!(ordered[0].folder && !ordered[2].folder);
}

/// Menu order: each folder followed by what it holds, depth first, each level in its order; an
/// item whose folder is missing, or that a loop of folders hides, is left out (RC-080-50).
#[test]
fn the_menu_lists_each_folder_before_what_it_holds() {
    let item = |id: i64, parent: Option<i64>| HotlistItem {
        id,
        parent_id: parent,
        name: format!("n{id}"),
        location: None,
        target: None,
        sort_order: 0,
    };
    // As `rc-db` lists them: level by level.
    let items = vec![
        item(1, None),
        item(2, None),
        item(3, Some(1)),
        item(4, Some(1)),
        item(5, Some(3)),
        item(6, Some(99)),
        item(7, Some(8)),
        item(8, Some(7)),
    ];
    let order: Vec<i64> = menu_order(items).iter().map(|entry| entry.id).collect();
    assert_eq!(order, [1, 3, 5, 4, 2]);
}
