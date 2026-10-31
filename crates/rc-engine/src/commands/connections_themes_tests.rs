//! A connection's themes through `Engine::dispatch` (RC-100-51): a theme of each mode is taken,
//! one of the other mode or unknown is refused, an empty text clears, a theme-only change keeps
//! the session.

use rc_db::{Db, StoredTheme};
use rc_vfs::VfsPath;
use tempfile::TempDir;

use crate::{
    Command, Connection, ConnectionDraft, ConnectionId, ConnectionPatch, Engine, EngineError,
    Outcome, SaveConnectionRequest, SaveFields,
};

async fn engine() -> (TempDir, Engine) {
    let root = tempfile::tempdir().expect("temp dir");
    let db = Db::open_in_memory().await.expect("db");
    let engine = Engine::headless(VfsPath::local(root.path()))
        .await
        .expect("engine");
    assert!(engine.attach_db(db));
    (root, engine)
}

fn themed(light: Option<&str>, dark: Option<&str>) -> ConnectionDraft {
    ConnectionDraft {
        theme_light: light.map(str::to_owned),
        theme_dark: dark.map(str::to_owned),
        ..ConnectionDraft::new("web", "example.org")
    }
}

async fn create(engine: &Engine, draft: ConnectionDraft) -> Result<Connection, EngineError> {
    let command = Command::CreateConnection {
        folder_id: None,
        connection: draft,
    };
    match engine.dispatch(command).await? {
        Outcome::Connection { connection } => Ok(connection),
        other => panic!("a connection, got {other:?}"),
    }
}

async fn update(
    engine: &Engine,
    id: ConnectionId,
    patch: ConnectionPatch,
) -> Result<Connection, EngineError> {
    match engine
        .dispatch(Command::UpdateConnection { id, patch })
        .await?
    {
        Outcome::Connection { connection } => Ok(connection),
        other => panic!("a connection, got {other:?}"),
    }
}

#[tokio::test]
async fn a_theme_of_each_mode_is_taken() {
    let (_root, engine) = engine().await;
    let made = create(&engine, themed(Some("nord-light"), Some("solarized-dark")))
        .await
        .expect("created");
    assert_eq!(made.theme_light.as_deref(), Some("nord-light"));
    assert_eq!(made.theme_dark.as_deref(), Some("solarized-dark"));
}

#[tokio::test]
async fn a_theme_of_the_other_mode_or_unknown_is_refused() {
    let (_root, engine) = engine().await;
    let wrong = create(&engine, themed(Some("nord-dark"), None))
        .await
        .expect_err("a dark theme for the light mode");
    assert_eq!(wrong.code(), "theme_wrong_mode");
    let wrong = create(&engine, themed(None, Some("nord-light")))
        .await
        .expect_err("a light theme for the dark mode");
    assert_eq!(wrong.code(), "theme_wrong_mode");
    let unknown = create(&engine, themed(Some("own-missing"), None))
        .await
        .expect_err("unknown");
    assert_eq!(unknown.code(), "theme_unknown");
    let made = create(&engine, themed(None, None)).await.expect("none");
    let refused = update(
        &engine,
        made.id,
        ConnectionPatch {
            theme_dark: Some("classic-light".into()),
            ..ConnectionPatch::default()
        },
    )
    .await
    .expect_err("wrong mode");
    assert_eq!(refused.code(), "theme_wrong_mode");
}

#[tokio::test]
async fn an_own_theme_is_taken_and_an_empty_text_clears() {
    let (_root, engine) = engine().await;
    let db = engine.db().expect("db").clone();
    db.save_theme(&StoredTheme {
        id: "own-night".into(),
        name: "Night".into(),
        dark: true,
        accent: String::new(),
        tokens: "{}".into(),
        saved_at: String::new(),
    })
    .await
    .expect("theme");
    let made = create(&engine, themed(None, Some("own-night")))
        .await
        .expect("created");
    assert_eq!(made.theme_dark.as_deref(), Some("own-night"));
    let cleared = update(
        &engine,
        made.id,
        ConnectionPatch {
            theme_dark: Some(String::new()),
            ..ConnectionPatch::default()
        },
    )
    .await
    .expect("cleared");
    assert_eq!(cleared.theme_dark, None);
}

#[tokio::test]
async fn the_form_s_save_checks_the_themes_too() {
    let (_root, engine) = engine().await;
    let request = |draft| SaveConnectionRequest {
        fields: SaveFields::Create {
            folder_id: None,
            connection: draft,
        },
        secrets: Vec::new(),
        login_script: None,
        favourite: None,
        credential: None,
    };
    let refused = engine
        .dispatch(Command::SaveConnection(request(themed(
            Some("fluent-dark"),
            None,
        ))))
        .await
        .expect_err("wrong mode");
    assert_eq!(refused.code(), "theme_wrong_mode");
    let saved = engine
        .dispatch(Command::SaveConnection(request(themed(
            Some("classic-light"),
            Some("classic-dark"),
        ))))
        .await
        .expect("saved");
    let Outcome::Connection { connection } = saved else {
        panic!("a connection, got {saved:?}");
    };
    assert_eq!(connection.theme_light.as_deref(), Some("classic-light"));
}
