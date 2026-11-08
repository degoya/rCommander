//! The column view through the commands: the automatic switch by folder mask, a mode by hand,
//! the comments view and when the comments are read.

use std::path::{MAIN_SEPARATOR, Path};

use serde_json::json;

use crate::{
    ColumnSetId, ColumnSetSpec, ColumnSetsOutcome, Command, Engine, Field, ListMode, Outcome,
    PanelId, SortKey, test_support,
};

const LEFT: Option<PanelId> = Some(PanelId::Left);

async fn set_with_mask(engine: &Engine, name: &str, folder: &Path) -> ColumnSetId {
    let mut spec = ColumnSetSpec::of(&[Field::Name, Field::Created, Field::Comment]);
    spec.masks = vec![format!("{}{MAIN_SEPARATOR}*", folder.display())];
    let command = Command::CreateColumnSet {
        name: name.into(),
        spec,
    };
    match engine.dispatch(command).await.expect("saved") {
        Outcome::ColumnSets(outcome) => match *outcome {
            ColumnSetsOutcome::Saved { id } => id,
            other => panic!("saved, got {other:?}"),
        },
        other => panic!("column sets, got {other:?}"),
    }
}

async fn go(engine: &Engine, path: &Path) -> ListMode {
    let command = Command::Navigate {
        panel: LEFT,
        path: path.display().to_string(),
    };
    test_support::run_panel(engine, command).await.mode
}

async fn setting(engine: &Engine, key: &str, value: serde_json::Value) {
    engine
        .dispatch(Command::SetSetting {
            key: key.into(),
            value,
        })
        .await
        .expect("stored");
}

#[tokio::test]
async fn a_folder_mask_switches_the_panel_and_the_default_decides_outside() {
    let (root, engine) =
        test_support::engine_with_db(&[("photos/2024/a.jpg", "a"), ("docs/b.txt", "b")]).await;
    let photos = root.path().join("photos");
    let id = set_with_mask(&engine, "Photos", &photos).await;
    let custom = ListMode::Custom { set: id };

    assert_eq!(go(&engine, &photos).await, custom, "the folder itself");
    assert_eq!(go(&engine, &photos.join("2024")).await, custom, "below it");
    assert_eq!(
        go(&engine, &root.path().join("docs")).await,
        custom,
        "unchanged outside, as Total Commander's <default>"
    );

    setting(&engine, "columns.auto_default", json!("full")).await;
    assert_eq!(go(&engine, &photos).await, custom);
    assert_eq!(go(&engine, root.path()).await, ListMode::Full);

    setting(&engine, "columns.auto_switch", json!(false)).await;
    assert_eq!(
        go(&engine, &photos).await,
        ListMode::Full,
        "the switch is off"
    );
    engine
        .dispatch(Command::ToggleAutoColumns {})
        .await
        .expect("switched");
    assert_eq!(go(&engine, &photos).await, custom, "and on again");
}

#[tokio::test]
async fn a_mode_by_hand_stays_until_a_folder_of_another_set() {
    let (root, engine) =
        test_support::engine_with_db(&[("photos/2024/a.jpg", "a"), ("code/x.rs", "x")]).await;
    let photos = root.path().join("photos");
    set_with_mask(&engine, "Photos", &photos).await;
    let code = set_with_mask(&engine, "Code", &root.path().join("code")).await;
    setting(&engine, "columns.auto_default", json!("full")).await;

    go(&engine, &photos).await;
    let brief = Command::SetListMode {
        panel: LEFT,
        mode: ListMode::Brief,
    };
    test_support::run_panel(&engine, brief).await;
    assert_eq!(go(&engine, &photos.join("2024")).await, ListMode::Brief);
    assert_eq!(
        go(&engine, root.path()).await,
        ListMode::Brief,
        "leaving keeps it"
    );
    assert_eq!(
        go(&engine, &root.path().join("code")).await,
        ListMode::Custom { set: code },
        "another set's folder ends it"
    );
}

#[tokio::test]
async fn the_comments_are_read_only_while_the_mode_shows_them() {
    let (root, engine) = test_support::engine_with_db(&[
        ("a.txt", "a"),
        ("b.txt", "b"),
        ("descript.ion", "a.txt zebra\r\nb.txt apple\r\n"),
    ])
    .await;
    assert!(
        engine.panel(PanelId::Left).columns.comments.is_none(),
        "the full view reads no descript.ion"
    );

    let shown = test_support::run_panel(&engine, Command::ToggleComments { panel: LEFT }).await;
    assert_eq!(shown.mode, ListMode::Comments);
    let panel = engine.panel(PanelId::Left);
    assert_eq!(panel.comment("a.txt"), Some("zebra"));
    assert_eq!(panel.comment("B.TXT"), Some("apple"));

    let sort = Command::Sort {
        panel: LEFT,
        key: SortKey::Comment,
        descending: None,
    };
    test_support::run_panel(&engine, sort).await;
    let names: Vec<String> = engine
        .panel(PanelId::Left)
        .entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    assert_eq!(
        names,
        ["descript.ion", "b.txt", "a.txt"],
        "by comment, none first"
    );

    let path = root.path().join("a.txt").display().to_string();
    engine
        .dispatch(Command::SetComment {
            panel: LEFT,
            path: Some(path),
            comment: Some("aardvark".into()),
        })
        .await
        .expect("set");
    let panel = engine.panel(PanelId::Left);
    assert_eq!(panel.comment("a.txt"), Some("aardvark"), "the row follows");
    assert_eq!(panel.entries[1].name, "a.txt", "and sorts again");

    let back = test_support::run_panel(&engine, Command::ToggleComments { panel: LEFT }).await;
    assert_eq!(back.mode, ListMode::Full, "back to the mode before");
    assert!(engine.panel(PanelId::Left).columns.comments.is_none());
}

#[tokio::test]
async fn a_set_with_a_comment_column_reads_them_and_a_set_by_hand_is_checked() {
    let (root, engine) =
        test_support::engine_with_db(&[("a.txt", "a"), ("descript.ion", "a.txt note\r\n")]).await;
    let id = set_with_mask(&engine, "Notes", &root.path().join("elsewhere")).await;
    let shown = test_support::run_panel(
        &engine,
        Command::SetListMode {
            panel: LEFT,
            mode: ListMode::Custom { set: id },
        },
    )
    .await;
    assert_eq!(shown.mode, ListMode::Custom { set: id });
    assert_eq!(engine.panel(PanelId::Left).comment("a.txt"), Some("note"));
    let unknown = Command::SetListMode {
        panel: LEFT,
        mode: ListMode::Custom { set: id + 100 },
    };
    let refused = engine.dispatch(unknown).await.expect_err("refused");
    assert_eq!(refused.code(), "column_set_unknown");
}
