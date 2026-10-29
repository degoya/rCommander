use std::{
    fmt::Write as _,
    path::PathBuf,
    sync::{Arc, Mutex, PoisonError},
};

use rc_db::{DB_FILE, Db, DbError};
use rc_term::ShellProfile;
use serde_json::{Value, json};
use tracing_subscriber::layer::SubscriberExt as _;

use super::*;
use crate::test_support;
use crate::{Command, CommandClass, Engine, EngineEvent, Outcome, spec};

/// The warnings logged while it runs, each as its fields.
#[derive(Clone, Default)]
struct Warnings(Arc<Mutex<Vec<String>>>);

impl Warnings {
    fn lines(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Warnings {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if *event.metadata().level() != tracing::Level::WARN {
            return;
        }
        let mut line = String::new();
        event.record(
            &mut |field: &tracing::field::Field, value: &dyn std::fmt::Debug| {
                let _ = write!(line, "{}={value:?} ", field.name());
            },
        );
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(line);
    }
}

pub(super) fn logged<T>(run: impl FnOnce() -> T) -> (T, Vec<String>) {
    let warnings = Warnings::default();
    let result = tracing::subscriber::with_default(
        tracing_subscriber::registry().with(warnings.clone()),
        run,
    );
    (result, warnings.lines())
}

pub(super) fn theme() -> &'static SettingSpec {
    setting(THEME_KEY).expect("the theme is a setting")
}

pub(super) async fn engine_with(db: Db) -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = test_support::headless(dir.path()).await;
    assert!(engine.attach_db(db));
    (dir, engine)
}

fn shell(id: &str) -> ShellProfile {
    ShellProfile {
        id: id.to_owned(),
        label_key: format!("terminal.shell.{id}"),
        program: PathBuf::from(id),
        args: Vec::new(),
        distro: None,
    }
}

#[test]
fn the_schema_is_consistent() {
    let mut keys: Vec<&str> = SETTINGS.iter().map(|spec| spec.key).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), SETTINGS.len(), "keys are unique");
    for spec in SETTINGS {
        assert_eq!(
            spec.check(&spec.default_value()),
            Ok(spec.default_value()),
            "{}: the default is valid",
            spec.key
        );
        assert!(spec.label.starts_with("setting."), "{}", spec.key);
        assert_eq!(spec.help, format!("{}.help", spec.label), "{}", spec.key);
        assert_eq!(spec.protected, is_protected(spec.key), "{}", spec.key);
    }
}

#[test]
fn mcp_and_vault_keys_are_protected_schema_row_or_not() {
    for key in [
        "mcp.token",
        "mcp.policy",
        "mcp.http.enabled",
        "mcp.http.port",
        "mcp.ask.headless_code",
        "vault.idle_lock_minutes",
        "vault.max_unlocked_hours",
        "vault.lock_on_session_lock",
        "vault.wincred_source",
        "vault.shared_credentials",
    ] {
        assert!(is_protected(key), "{key}");
    }
    assert!(!is_protected(THEME_KEY));
    assert!(!is_protected(SHELL_KEY));
}

#[test]
fn values_are_checked_against_their_row() {
    assert_eq!(theme().check(&json!("dark")), Ok(json!("dark")));
    assert!(theme().check(&json!("blue")).is_err());
    assert!(theme().check(&json!(1)).is_err());

    let idle = setting(rc_vault::IDLE_LOCK_KEY).expect("a setting");
    assert_eq!(idle.check(&json!(0)), Ok(json!(0)));
    assert!(idle.check(&json!(-1)).is_err());
    assert!(idle.check(&json!(1441)).is_err());

    let shell = setting(SHELL_KEY).expect("a setting");
    assert_eq!(shell.check(&json!("pwsh")), Ok(json!("pwsh")));
    assert!(shell.check(&json!("pwsh; rm -rf /")).is_err());

    let start = setting(LEFT_START_KEY).expect("a setting");
    let absolute = std::env::temp_dir().display().to_string();
    assert_eq!(start.check(&json!(absolute)), Ok(json!(absolute)));
    assert_eq!(start.check(&json!("")), Ok(json!("")));
    assert!(start.check(&json!("relative/dir")).is_err());
    assert!(start.check(&json!("/tmp/a\nb")).is_err());
}

/// The viewer's size reaches 6 to 72, as its text size keys do (RC-020-12).
#[test]
fn a_font_size_outside_its_range_is_refused_and_a_font_is_a_plain_name() {
    for (key, min, max) in [
        (LIST_FONT_SIZE_KEY, 8, 24),
        (TERMINAL_FONT_SIZE_KEY, 8, 24),
        (VIEWER_FONT_SIZE_KEY, 6, 72),
    ] {
        let size = setting(key).expect("a setting");
        assert_eq!(size.default_value(), json!(13), "{key}");
        assert_eq!(size.check(&json!(min)), Ok(json!(min)), "{key}");
        assert_eq!(size.check(&json!(max)), Ok(json!(max)), "{key}");
        assert!(size.check(&json!(min - 1)).is_err(), "{key}");
        assert!(size.check(&json!(max + 1)).is_err(), "{key}");
        assert!(size.check(&json!("12")).is_err(), "{key}");
    }
    for key in [LIST_FONT_KEY, TERMINAL_FONT_KEY, VIEWER_FONT_KEY] {
        let font = setting(key).expect("a setting");
        assert_eq!(font.type_name(), "font");
        assert!(!is_protected(key), "{key}: an agent may change it");
        assert_eq!(font.default_value(), json!(""), "{key}");
        let mono = json!("Cascadia Mono");
        assert_eq!(font.check(&mono), Ok(mono.clone()), "{key}");
        assert!(font.check(&json!("Consolas\n")).is_err(), "{key}");
        assert!(font.check(&json!(" Consolas")).is_err(), "{key}");
        assert!(font.check(&json!("x".repeat(129))).is_err(), "{key}");
        assert!(font.check(&json!(13)).is_err(), "{key}");
    }
}

/// RC-020-12: the text size keys are on by default and change the window alone.
#[test]
fn the_text_zoom_is_on_and_stays_with_its_window_by_default() {
    let zoom = setting(TEXT_ZOOM_KEY).expect("a setting");
    assert_eq!(zoom.default_value(), json!(true));
    assert!(!is_protected(TEXT_ZOOM_KEY), "an agent may change it");
    let keep = setting(TEXT_ZOOM_KEEP_KEY).expect("a setting");
    assert_eq!(keep.default_value(), json!("window"));
    assert_eq!(keep.check(&json!("setting")), Ok(json!("setting")));
    assert!(keep.check(&json!("everywhere")).is_err());
}

#[test]
fn the_dos_extensions_are_a_plain_list_of_words() {
    let list = setting(DOS_EXTENSIONS_KEY).expect("a setting");
    assert_eq!(list.type_name(), "extensions");
    assert!(!is_protected(DOS_EXTENSIONS_KEY), "an agent may change it");
    assert_eq!(list.default_value(), json!("nfo diz"));
    assert_eq!(
        list.check(&json!("*.NFO, .diz;asc")),
        Ok(json!("nfo diz asc"))
    );
    assert_eq!(list.check(&json!("")), Ok(json!("")));
    assert!(list.check(&json!("nfo/../x")).is_err());
    assert!(list.check(&json!("x".repeat(17))).is_err());
    assert!(list.check(&json!("a ".repeat(65))).is_err());
    assert!(list.check(&json!(1)).is_err());
}

#[test]
fn an_invalid_stored_value_reads_as_the_default_with_a_warning() {
    let (value, warnings) = logged(|| effective(theme(), Ok(Some(json!(42)))));
    assert_eq!(value, json!("system"));
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("appearance.theme"), "{warnings:?}");
    assert!(
        !warnings[0].contains("42"),
        "the value is not logged: {warnings:?}"
    );

    let unreadable = serde_json::from_str::<Value>("{").expect_err("not json");
    let stored = Err(DbError::Json {
        key: THEME_KEY.to_owned(),
        source: unreadable,
    });
    let (value, warnings) = logged(|| effective(theme(), stored));
    assert_eq!(value, json!("system"));
    assert_eq!(warnings.len(), 1, "{warnings:?}");

    let (value, warnings) = logged(|| effective(theme(), Ok(None)));
    assert_eq!(value, json!("system"));
    assert!(warnings.is_empty(), "nothing stored is no warning");
}

#[tokio::test]
async fn settings_get_reads_every_row_with_the_default_for_a_bad_value() {
    let db = Db::open_in_memory().await.expect("db");
    db.set(THEME_KEY, &42).await.expect("stored");
    db.set(SHELL_KEY, "bash").await.expect("stored");
    let (_dir, engine) = engine_with(db).await;

    let Outcome::Settings { settings } = engine
        .dispatch(Command::GetSettings { key: None })
        .await
        .expect("read")
    else {
        panic!("settings");
    };
    assert_eq!(settings.len(), SETTINGS.len());
    let theme = settings.iter().find(|s| s.key == THEME_KEY).expect("theme");
    assert_eq!(theme.value, json!("system"));
    assert_eq!(theme.choices, ["system", "light", "dark"]);
    let shell = settings.iter().find(|s| s.key == SHELL_KEY).expect("shell");
    assert_eq!(shell.value, json!("bash"));
    let idle = settings
        .iter()
        .find(|s| s.key == rc_vault::IDLE_LOCK_KEY)
        .expect("idle lock");
    assert!(idle.protected);
    assert_eq!((idle.min, idle.max), (Some(0), Some(1440)));

    let unknown = engine
        .dispatch(Command::GetSettings {
            key: Some("no.such".into()),
        })
        .await
        .expect_err("unknown");
    assert_eq!(unknown.code(), "setting_unknown");
}

#[tokio::test]
async fn settings_set_stores_announces_and_survives_a_restart() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(DB_FILE);
    let db = Db::open(&path).await.expect("db");
    let (_root, engine) = engine_with(db.clone()).await;
    let mut events = engine.subscribe();

    let answer = engine
        .dispatch(Command::SetSetting {
            key: THEME_KEY.into(),
            value: json!("dark"),
        })
        .await
        .expect("stored");
    let Outcome::Settings { settings } = answer else {
        panic!("settings");
    };
    assert_eq!(settings[0].value, json!("dark"));
    assert_eq!(
        events.try_recv().expect("an event"),
        EngineEvent::SettingChanged {
            key: THEME_KEY.into()
        }
    );
    db.close().await;

    let reopened = Db::open(&path).await.expect("reopened");
    assert_eq!(
        stored_setting(&reopened, THEME_KEY).await,
        Some(json!("dark"))
    );
}

#[tokio::test]
async fn settings_set_refuses_invalid_and_unknown_values() {
    let db = Db::open_in_memory().await.expect("db");
    let (_dir, engine) = engine_with(db.clone()).await;
    let invalid = engine
        .dispatch(Command::SetSetting {
            key: THEME_KEY.into(),
            value: json!("purple"),
        })
        .await
        .expect_err("invalid");
    assert_eq!(invalid.code(), "setting_invalid");
    let unknown = engine
        .dispatch(Command::SetSetting {
            key: "panels.font".into(),
            value: json!(12),
        })
        .await
        .expect_err("unknown");
    assert_eq!(unknown.code(), "setting_unknown");
    assert_eq!(db.get::<Value>(THEME_KEY).await.expect("read"), None);
}

/// Neither the window's dispatch nor MCP's can change an MCP or vault setting through the
/// command; only [`Engine::set_protected_setting`] can.
#[tokio::test]
async fn settings_set_never_changes_mcp_or_vault_settings() {
    let db = Db::open_in_memory().await.expect("db");
    let (_dir, engine) = engine_with(db.clone()).await;
    let attempts = [
        ("mcp.token", json!("0000")),
        (
            "mcp.policy",
            json!({ "destructive": "allowed", "exec": "allowed" }),
        ),
        ("mcp.http.enabled", json!(false)),
        ("mcp.http.port", json!(8080)),
        ("vault.idle_lock_minutes", json!(0)),
        ("vault.max_unlocked_hours", json!(0)),
        ("vault.lock_on_session_lock", json!(false)),
        ("vault.wincred_source", json!(false)),
        ("vault.shared_credentials", json!(false)),
        ("mcp.ask.headless_code", json!(true)),
    ];
    for (key, value) in attempts {
        for via_mcp in [false, true] {
            let command = Command::SetSetting {
                key: key.into(),
                value: value.clone(),
            };
            let refused = if via_mcp {
                engine.dispatch_mcp(command).await
            } else {
                engine.dispatch(command).await
            };
            assert_eq!(
                refused.expect_err("refused").code(),
                "setting_protected",
                "{key}"
            );
        }
        assert_eq!(db.get::<Value>(key).await.expect("read"), None, "{key}");
    }

    let stored = engine
        .set_protected_setting("mcp.http.enabled", &json!(false))
        .await
        .expect("the window may");
    assert_eq!(stored.value, json!(false));
    assert_eq!(
        db.get::<bool>("mcp.http.enabled").await.expect("read"),
        Some(false)
    );
}

/// RC-100-10: a plugin document's colours are on by default (on the Plugins page since
/// RC-100-09), and like every `plugins.*` and
/// `plugin.*` key only the window switches them - never `settings.set`, from MCP or not.
#[tokio::test]
async fn the_token_colors_are_on_and_settings_set_refuses_every_plugin_key() {
    let colors = setting(VIEWER_TOKEN_COLORS_KEY).expect("a setting");
    assert_eq!(colors.default_value(), json!(true));
    assert_eq!(colors.page, SettingPage::Plugins);
    for key in [
        VIEWER_TOKEN_COLORS_KEY,
        "plugins.enabled",
        "plugin.json-viewer.indent",
    ] {
        assert!(is_protected(key), "{key}");
    }

    let db = Db::open_in_memory().await.expect("db");
    let (_dir, engine) = engine_with(db.clone()).await;
    for via_mcp in [false, true] {
        let command = Command::SetSetting {
            key: VIEWER_TOKEN_COLORS_KEY.into(),
            value: json!(false),
        };
        let refused = if via_mcp {
            engine.dispatch_mcp(command).await
        } else {
            engine.dispatch(command).await
        };
        assert_eq!(refused.expect_err("refused").code(), "setting_protected");
    }
    assert_eq!(
        db.get::<Value>(VIEWER_TOKEN_COLORS_KEY)
            .await
            .expect("read"),
        None
    );
    let stored = engine
        .set_protected_setting(VIEWER_TOKEN_COLORS_KEY, &json!(false))
        .await
        .expect("the window may");
    assert_eq!(stored.value, json!(false));
}

#[test]
fn the_commands_have_the_classes_their_effects_need() {
    assert_eq!(
        spec("settings.get").map(|s| s.class),
        Some(CommandClass::Read)
    );
    assert_eq!(
        spec("settings.set").map(|s| s.class),
        Some(CommandClass::Write)
    );
}

#[test]
fn the_default_shell_is_the_setting_when_installed_else_the_first() {
    let shells = || vec![shell("pwsh"), shell("cmd")];
    let id = |picked: Option<ShellProfile>| picked.map(|shell| shell.id);
    assert_eq!(id(pick_shell(shells(), Some("cmd"))), Some("cmd".into()));
    assert_eq!(id(pick_shell(shells(), Some("fish"))), Some("pwsh".into()));
    assert_eq!(id(pick_shell(shells(), Some(""))), Some("pwsh".into()));
    assert_eq!(id(pick_shell(shells(), None)), Some("pwsh".into()));
    assert_eq!(id(pick_shell(Vec::new(), Some("cmd"))), None);
}

/// The owner's test list (RC-080-14): untouched, the Transfers page means two transfers per
/// server and four in all; a value set there reaches the queue with the next copy.
#[tokio::test]
async fn the_transfer_limits_are_two_per_server_and_four_in_all_until_set() {
    let db = Db::open_in_memory().await.expect("db");
    let (_dir, engine) = engine_with(db).await;
    engine.ops().set_transfer_limits(4096, 1, 1);
    engine.apply_transfer_settings().await;
    let config = engine.ops().config();
    assert_eq!((config.per_connection, config.max_running), (2, 4));
    assert_eq!(config.buffer_size, rc_ops::DEFAULT_BUFFER_SIZE);

    for (key, value) in [(TRANSFER_PER_CONNECTION_KEY, 3), (TRANSFER_TOTAL_KEY, 6)] {
        engine
            .dispatch(Command::SetSetting {
                key: key.into(),
                value: json!(value),
            })
            .await
            .expect("stored");
    }
    engine.apply_transfer_settings().await;
    let config = engine.ops().config();
    assert_eq!((config.per_connection, config.max_running), (3, 6));
}

#[tokio::test]
async fn a_switch_reads_stored_else_its_default_and_off_for_no_switch() {
    let db = Db::open_in_memory().await.expect("db");
    assert!(stored_bool(&db, MENU_BAR_KEY).await, "default on");
    assert!(!stored_bool(&db, OWNER_COLUMN_KEY).await, "default off");
    db.set(MENU_BAR_KEY, &false).await.expect("stored");
    db.set(OWNER_COLUMN_KEY, "yes").await.expect("stored");
    assert!(!stored_bool(&db, MENU_BAR_KEY).await, "stored off");
    assert!(
        !stored_bool(&db, OWNER_COLUMN_KEY).await,
        "invalid: default"
    );
    assert!(!stored_bool(&db, THEME_KEY).await, "no switch");
    assert!(!stored_bool(&db, "no.such.key").await, "unknown");
}
