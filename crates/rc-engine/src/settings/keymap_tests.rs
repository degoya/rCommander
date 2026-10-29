use serde_json::json;

use super::*;
use crate::{
    Chord, Command, CommandClass, ConfigExportRequest, ConfigImportRequest, EngineEvent, PanelView,
    exchange::{ExportParts, RowStatus},
    spec,
    test_support::{engine_in, headless, headless_with_db},
};

fn chord(text: &str) -> Chord {
    text.parse().expect("a chord")
}

fn rows_of(outcome: Result<Outcome, EngineError>) -> (Vec<crate::KeymapRow>, Vec<String>) {
    match outcome {
        Ok(Outcome::Keymap { rows, stale }) => (rows, stale),
        other => panic!("the keymap, got {other:?}"),
    }
}

#[test]
fn reading_is_an_agents_and_moving_a_key_a_persons() {
    assert_eq!(
        spec("keymap.list").map(|s| s.class),
        Some(CommandClass::Read)
    );
    assert_eq!(
        spec("keymap.set").map(|s| s.class),
        Some(CommandClass::GuiOnly)
    );
    assert!(super::super::is_protected(KEYMAP_KEY));
}

#[tokio::test]
async fn without_a_database_the_standard_is_in_force() {
    let dir = tempfile::tempdir().expect("dir");
    let engine = headless(dir.path()).await;
    assert_eq!(engine.keymap().await, Keymap::standard());
    let (rows, stale) = rows_of(engine.dispatch(Command::ListKeymap).await);
    assert!(stale.is_empty());
    assert_eq!(rows, Keymap::standard().rows());
}

/// The acceptance's example: F5 to Ctrl+K, stored, announced, in force after a restart, back
/// to the standard.
#[tokio::test]
async fn a_moved_key_is_stored_announced_and_reset() {
    let dir = tempfile::tempdir().expect("dir");
    let engine = engine_in(dir.path()).await;
    let mut events = engine.subscribe();
    let (rows, _) = rows_of(
        engine
            .dispatch(Command::SetKeymap {
                overrides: "F5= ctrl+k=copy".into(),
            })
            .await,
    );
    assert_eq!(
        events.try_recv().expect("an event"),
        EngineEvent::SettingChanged {
            key: KEYMAP_KEY.into()
        }
    );
    let copy = rows
        .iter()
        .find(|row| row.name == "copy")
        .expect("the copy");
    assert_eq!((copy.keys.as_str(), copy.changed), ("Ctrl+K", true));
    let db = engine.db().expect("db").clone();
    assert_eq!(
        stored_setting(&db, KEYMAP_KEY).await,
        Some(json!("F5= Ctrl+K=copy")),
        "kept as the keymap writes it"
    );

    // A restart: a new engine on the same database.
    let again = headless(dir.path()).await;
    assert!(again.attach_db(db));
    let keymap = again.keymap().await;
    let copy = Keymap::standard()
        .lookup(&chord("F5"), PanelView::Files)
        .cloned();
    assert_eq!(
        keymap.lookup(&chord("Ctrl+K"), PanelView::Files).cloned(),
        copy
    );
    assert_eq!(keymap.lookup(&chord("F5"), PanelView::Files), None);

    rows_of(
        again
            .dispatch(Command::SetKeymap {
                overrides: String::new(),
            })
            .await,
    );
    assert_eq!(again.keymap().await, Keymap::standard());
}

/// Neither `settings.set` nor an MCP call moves a key, and the window's own keys never reach
/// the setting.
#[tokio::test]
async fn only_the_window_moves_a_key_and_never_a_locked_one() {
    let dir = tempfile::tempdir().expect("dir");
    let engine = headless_with_db(dir.path()).await;
    let refused = engine
        .dispatch(Command::SetSetting {
            key: KEYMAP_KEY.into(),
            value: json!("Ctrl+K=copy"),
        })
        .await
        .expect_err("protected");
    assert_eq!(refused.code(), "setting_protected");
    let refused = engine
        .dispatch_mcp(Command::SetKeymap {
            overrides: "Ctrl+K=copy".into(),
        })
        .await
        .expect_err("GuiOnly");
    assert_eq!(refused.code(), "gui_only");
    for overrides in ["Tab=copy", "Escape=", "Ctrl+V=copy", "F5=Copy", "nonsense"] {
        let refused = engine
            .dispatch(Command::SetKeymap {
                overrides: overrides.into(),
            })
            .await
            .expect_err("invalid");
        assert_eq!(refused.code(), "setting_invalid", "{overrides}");
    }
    assert_eq!(engine.keymap().await, Keymap::standard());
    let (rows, _) = rows_of(engine.dispatch_mcp(Command::ListKeymap).await);
    assert!(!rows.is_empty(), "an agent reads the keys");
}

/// A row a newer standard no longer has stays stored but is not in force; `keymap.list` says
/// which.
#[tokio::test]
async fn a_stale_deviation_is_listed_and_left_out() {
    let dir = tempfile::tempdir().expect("dir");
    let engine = headless_with_db(dir.path()).await;
    let (rows, stale) = rows_of(
        engine
            .dispatch(Command::SetKeymap {
                overrides: "Ctrl+K=renamed_since Ctrl+J=copy".into(),
            })
            .await,
    );
    assert_eq!(stale, ["Ctrl+K=renamed_since"]);
    assert!(
        rows.iter()
            .any(|row| row.keys == "Ctrl+J" && row.name == "copy")
    );
    assert!(!rows.iter().any(|row| row.keys == "Ctrl+K"));
}

/// The deviations go with the settings of an export; an import writes them only when ticked,
/// as every protected setting.
#[tokio::test]
async fn the_deviations_travel_with_an_export() {
    let from = tempfile::tempdir().expect("dir");
    let engine = engine_in(from.path()).await;
    rows_of(
        engine
            .dispatch(Command::SetKeymap {
                overrides: "F5= Ctrl+K=copy".into(),
            })
            .await,
    );
    let path = from.path().join("keys.rcexport");
    let export = Command::ExportConfig(ConfigExportRequest {
        path: path.display().to_string(),
        parts: ExportParts {
            settings: true,
            connections: false,
            trust: false,
            hotlist: false,
        },
        password: None,
        secrets: false,
        plain_secrets: false,
        overwrite: true,
    });
    engine.dispatch(export).await.expect("exported");
    let text = std::fs::read_to_string(&path).expect("written");
    assert!(text.contains("F5= Ctrl+K=copy"), "{text}");

    let to = tempfile::tempdir().expect("dir");
    let target = engine_in(to.path()).await;
    let import = |only: Option<Vec<String>>| {
        Command::ImportConfig(ConfigImportRequest {
            path: path.display().to_string(),
            password: None,
            dry_run: false,
            only,
        })
    };
    let Ok(Outcome::ConfigImported(report)) = target.dispatch(import(None)).await else {
        panic!("an import report");
    };
    let row = report
        .rows
        .iter()
        .find(|row| row.key == format!("setting:{KEYMAP_KEY}"))
        .expect("the keys' row");
    assert_eq!(
        row.status,
        RowStatus::Changed,
        "protected: not written unticked"
    );
    assert_eq!(target.keymap().await, Keymap::standard());
    let ticked = Some(vec![format!("setting:{KEYMAP_KEY}")]);
    target.dispatch(import(ticked)).await.expect("imported");
    assert_eq!(
        target
            .keymap()
            .await
            .key_of("copy")
            .map(ToString::to_string),
        Some("Ctrl+K".to_owned())
    );
}
