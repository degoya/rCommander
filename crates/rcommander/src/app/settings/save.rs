//! The dialog's work off the UI thread: reading what it shows, storing what changed, the token.
//! Every failure is a stable error code, which the dialog shows as its message.

use rc_db::Db;
use rc_engine::{
    Command, Engine, EngineError, Outcome,
    settings::{
        ALTERNATE_ROWS_KEY, DEFAULT_FONT_SIZE, DOS_TIGHT_LINES_KEY, KEYMAP_KEY, LIST_FONT_KEY,
        LIST_FONT_SIZE_KEY, MARK_HIDDEN_KEY, NAME_CUT_KEY, NAME_TIP_DELAY_KEY, TERMINAL_FONT_KEY,
        TERMINAL_FONT_SIZE_KEY, THEME_KEY, VIEWER_FONT_KEY, VIEWER_FONT_SIZE_KEY, is_protected,
        stored_bool, stored_setting,
    },
};
use rc_mcp::{McpToken, Policy};

use super::draft::{Changes, Loaded};
use crate::{
    mcp::{self, McpServer, McpStatus},
    theme::{
        ThemeChoice,
        fonts::{self, Family, Fonts, Names},
    },
};

fn code(error: &EngineError) -> &'static str {
    error.code()
}

/// Every setting, the retention limits, the policy and the shells.
pub async fn load(engine: Engine, db: Db) -> Result<Loaded, &'static str> {
    let Outcome::Settings { settings: values } = engine
        .dispatch(Command::GetSettings { key: None })
        .await
        .map_err(|error| code(&error))?
    else {
        return Err("database_error");
    };
    let Outcome::Retention { tables: retention } = engine
        .dispatch(Command::GetRetention)
        .await
        .map_err(|error| code(&error))?
    else {
        return Err("database_error");
    };
    // A policy that does not read shows the defaults; storing them repairs it.
    let policy = Policy::load(&db).await.unwrap_or_else(|error| {
        tracing::warn!(%error, "the MCP policy does not read; the dialog shows the defaults");
        Policy::default()
    });
    let shells = match engine.dispatch(Command::ListTerminals {}).await {
        Ok(Outcome::Terminals { shells, .. }) => shells.into_iter().map(|shell| shell.id).collect(),
        _ => Vec::new(),
    };
    Ok(Loaded {
        values,
        retention,
        policy,
        shells,
        fonts: installed_fonts().await,
    })
}

/// What storing the changes came to.
#[derive(Debug, Clone)]
pub struct Stored {
    /// The MCP server's new state when its switch or port changed: it restarts even when a
    /// later change failed, as the stored switch and port hold by then (RC-080-44).
    pub mcp: Option<McpStatus>,
    /// Why a change was not stored; those before it are.
    pub error: Option<&'static str>,
}

/// Stores `changes`, in order up to the first that fails; the MCP server restarts when its
/// switch or port was stored.
pub async fn store(engine: Engine, db: Db, server: McpServer, changes: Changes) -> Stored {
    let mut restart = false;
    let error = store_changes(&engine, &db, changes, &mut restart)
        .await
        .err();
    if let Some(code) = error {
        tracing::warn!(code, "a setting was not stored");
    }
    let mcp = if restart {
        let instance_file = rc_mcp::instance::default_path().ok();
        Some(mcp::restart(server, engine, db, instance_file).await)
    } else {
        None
    };
    Stored { mcp, error }
}

/// Stores `changes`; `restart` is raised once the MCP server's switch or port is stored.
async fn store_changes(
    engine: &Engine,
    db: &Db,
    changes: Changes,
    restart: &mut bool,
) -> Result<(), &'static str> {
    for (key, value) in changes.settings {
        // The keys go through their own command, which only the window may run (RC-100-21).
        if key == KEYMAP_KEY {
            let overrides = value.as_str().unwrap_or_default().to_owned();
            engine
                .dispatch(Command::SetKeymap { overrides })
                .await
                .map_err(|error| code(&error))?;
        } else if is_protected(key) {
            engine
                .set_protected_setting(key, &value)
                .await
                .map_err(|error| code(&error))?;
            // The server's switch and port; the other MCP settings are read per call.
            *restart |= key.starts_with("mcp.http.");
        } else {
            let command = Command::SetSetting {
                key: key.to_owned(),
                value,
            };
            engine
                .dispatch(command)
                .await
                .map_err(|error| code(&error))?;
        }
    }
    for (table, rows, days) in changes.retention {
        let command = Command::SetRetention(rc_engine::RetentionRequest {
            table,
            max_rows: Some(rows),
            max_age_days: Some(days),
        });
        engine
            .dispatch(command)
            .await
            .map_err(|error| code(&error))?;
    }
    if let Some(policy) = changes.policy {
        policy.store(db).await.map_err(|error| {
            tracing::warn!(%error, "the MCP policy was not stored");
            "database_error"
        })?;
    }
    Ok(())
}

/// The token, to copy.
pub async fn token(db: Db) -> Result<McpToken, &'static str> {
    // Without the error's text: a stored value that does not decode would be quoted in it.
    McpToken::load_or_create(&db).await.map_err(|_| {
        tracing::warn!("the MCP token does not read");
        "database_error"
    })
}

/// A new token in force: stored, and the server restarted with it.
pub async fn rotate(engine: Engine, db: Db, server: McpServer) -> Result<McpStatus, &'static str> {
    McpToken::rotate(&db).await.map_err(|_| {
        tracing::warn!("a new MCP token was not stored");
        "database_error"
    })?;
    let instance_file = rc_mcp::instance::default_path().ok();
    Ok(mcp::restart(server, engine, db, instance_file).await)
}

/// The stored theme.
pub async fn theme(db: Db) -> ThemeChoice {
    let value = stored_setting(&db, THEME_KEY).await;
    ThemeChoice::from_setting(value.as_ref().and_then(|v| v.as_str()).unwrap_or_default())
}

/// Whether every other row of the lists and tables is tinted; on unless switched off.
pub async fn alternate_rows(db: Db) -> bool {
    stored_bool(&db, ALTERNATE_ROWS_KEY).await
}

/// The installed font families, read off the async workers: the font database is locked.
async fn installed_fonts() -> Vec<Family> {
    tokio::task::spawn_blocking(fonts::installed)
        .await
        .unwrap_or_default()
}

/// The fonts of the lists, the terminal and the viewer (RC-010-17): each family the settings name
/// when it is installed, the area's default otherwise, at the stored size.
pub async fn fonts(db: Db) -> Fonts {
    let list = chosen(&db, LIST_FONT_KEY, LIST_FONT_SIZE_KEY).await;
    let terminal = chosen(&db, TERMINAL_FONT_KEY, TERMINAL_FONT_SIZE_KEY).await;
    let viewer = chosen(&db, VIEWER_FONT_KEY, VIEWER_FONT_SIZE_KEY).await;
    // The font database is asked only when a family is named.
    let installed = if [&list, &terminal, &viewer]
        .iter()
        .all(|(name, _)| name.is_empty())
    {
        Vec::new()
    } else {
        installed_fonts().await
    };
    let kind = |name: &str| {
        installed
            .iter()
            .find(|family| family.name == name)
            .map(|family| family.mono)
    };
    let resolve = |(name, size): &(String, i64), size_key, default| {
        fonts::resolve(name, *size, fonts::sizes_of(size_key), default, kind)
    };
    let viewer = resolve(&viewer, VIEWER_FONT_SIZE_KEY, Fonts::DEFAULT.viewer);
    Fonts {
        list: resolve(&list, LIST_FONT_SIZE_KEY, Fonts::DEFAULT.list),
        terminal: resolve(&terminal, TERMINAL_FONT_SIZE_KEY, Fonts::DEFAULT.terminal),
        viewer,
        names: names(&db).await,
        dos_block: dos_block(&db, viewer.font).await,
    }
}

/// The viewer font's full block, measured off the async workers, while `viewer.dos_tight_lines`
/// is on (the default); `None` when it is off or the font has no block.
async fn dos_block(db: &Db, font: iced::Font) -> Option<f32> {
    let tight = stored_bool(db, DOS_TIGHT_LINES_KEY).await;
    if !tight {
        return None;
    }
    tokio::task::spawn_blocking(move || fonts::block_height(font))
        .await
        .ok()
        .flatten()
}

/// How the lists shorten a name and how long its tooltip waits.
async fn names(db: &Db) -> Names {
    let cut = stored_setting(db, NAME_CUT_KEY).await;
    let delay = stored_setting(db, NAME_TIP_DELAY_KEY).await;
    let delay = delay.as_ref().and_then(|value| value.as_u64());
    Names {
        cut_end: cut.as_ref().and_then(|value| value.as_str()) != Some("middle"),
        tip_delay: delay.map_or(Names::DEFAULT.tip_delay, std::time::Duration::from_millis),
        mark_hidden: stored_bool(db, MARK_HIDDEN_KEY).await,
    }
}

/// The stored family (empty for the default) and size of one area.
async fn chosen(db: &Db, name_key: &str, size_key: &str) -> (String, i64) {
    let name = stored_setting(db, name_key).await;
    let size = stored_setting(db, size_key).await;
    (
        name.as_ref()
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_owned(),
        size.as_ref()
            .and_then(|value| value.as_i64())
            .unwrap_or(DEFAULT_FONT_SIZE),
    )
}
