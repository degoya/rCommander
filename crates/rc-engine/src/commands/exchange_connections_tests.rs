//! A connection's colour themes in an export and an import (RC-100-51): they go out with it; a
//! theme the target has not is left out with the hint `theme_missing`, the row still chosen; a
//! theme the same file brings counts as there in the preview.

use rc_db::StoredTheme;

use crate::{
    Command, ConnectionDraft, Engine, Outcome,
    commands::exchange_fixture::{export, import, report},
    exchange::{ExportParts, ExportedTheme, RowStatus},
    settings::themes::load,
    test_support,
};

use super::values::fit_themes;

const NIGHT: &str = "own-0badc0de";

async fn with_night(engine: &Engine) {
    engine
        .db()
        .expect("db")
        .save_theme(&StoredTheme {
            id: NIGHT.to_owned(),
            name: "Night".to_owned(),
            dark: true,
            accent: String::new(),
            tokens: "{}".to_owned(),
            saved_at: String::new(),
        })
        .await
        .expect("theme");
}

fn themed(light: &str, dark: &str) -> ConnectionDraft {
    ConnectionDraft {
        theme_light: Some(light.to_owned()),
        theme_dark: Some(dark.to_owned()),
        ..ConnectionDraft::new("web", "example.org")
    }
}

#[tokio::test]
async fn a_theme_not_here_is_left_out_and_said() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let here = load(engine.db().expect("db")).await;
    let (fitted, dropped) = fit_themes(&themed("nord-light", NIGHT), &here, &[]);
    assert!(dropped);
    assert_eq!(fitted.theme_light.as_deref(), Some("nord-light"));
    assert_eq!(fitted.theme_dark, None);
    let (fitted, dropped) = fit_themes(&themed("nord-dark", "nord-dark"), &here, &[]);
    assert!(dropped, "a dark theme for the light mode");
    assert_eq!(fitted.theme_light, None);
    assert_eq!(fitted.theme_dark.as_deref(), Some("nord-dark"));
    let incoming = [ExportedTheme {
        id: NIGHT.to_owned(),
        name: "Night".to_owned(),
        dark: true,
        accent: "system".to_owned(),
        tokens: Default::default(),
    }];
    let (fitted, dropped) = fit_themes(&themed("nord-light", NIGHT), &here, &incoming);
    assert!(!dropped, "the file brings it");
    assert_eq!(fitted.theme_dark.as_deref(), Some(NIGHT));
}

#[tokio::test]
async fn the_themes_go_out_and_a_missing_one_stays_behind() {
    let (source_root, source) = test_support::engine_with_db(&[]).await;
    with_night(&source).await;
    source
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: themed("nord-light", NIGHT),
        })
        .await
        .expect("created");
    let path = source_root.path().join("connections.rcexport");
    let parts = ExportParts {
        connections: true,
        ..ExportParts::default()
    };
    source
        .dispatch(export(&path, parts, None, false))
        .await
        .expect("exported");

    let (_target_root, target) = test_support::engine_with_db(&[]).await;
    let preview = report(&target, import(&path, None, true, None)).await;
    let row = preview
        .rows
        .iter()
        .find(|row| row.key.starts_with("connection:"))
        .expect("the connection's row");
    assert_eq!(row.status, RowStatus::New);
    assert_eq!(row.warning.as_deref(), Some("theme_missing"));
    assert!(row.selected, "only a hint: the connection still comes in");

    report(&target, import(&path, None, false, None)).await;
    let Outcome::ConnectionTree { tree } = target
        .dispatch(Command::ListConnections)
        .await
        .expect("tree")
    else {
        panic!("the tree");
    };
    let web = &tree.connections[0];
    assert_eq!(web.theme_light.as_deref(), Some("nord-light"));
    assert_eq!(web.theme_dark, None, "Night is not here");

    // Where the theme is, it comes in with the connection.
    let (_third_root, third) = test_support::engine_with_db(&[]).await;
    with_night(&third).await;
    let preview = report(&third, import(&path, None, true, None)).await;
    assert!(preview.rows.iter().all(|row| row.warning.is_none()));
    report(&third, import(&path, None, false, None)).await;
    let tree = third
        .db()
        .expect("db")
        .connection_tree()
        .await
        .expect("tree");
    assert_eq!(tree.connections[0].theme_dark.as_deref(), Some(NIGHT));
}
