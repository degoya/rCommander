//! Starting the window: the database, the engine on the start folders, the MCP server inside
//! the app, the stored window geometry and theme - then iced, as a daemon whose first window is
//! the main window and whose others are dialogs in windows of their own (RC-080-15).
//!
//! The engine and the MCP server live on a tokio runtime of their own that outlasts the window,
//! so the server keeps answering while iced owns the main thread; it stops after the window
//! closes, and its instance file goes with it. The way out is bounded (`exit.rs`, RC-080-58).

use std::{
    error::Error,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use iced::{Task, system, window};
use rc_db::Db;
use rc_engine::{
    Command, Engine, EngineError, Keymap, Outcome, VfsPath,
    settings::{LEFT_START_KEY, RIGHT_START_KEY, THEME_KEY, stored_setting},
};
use rc_vault::Vault;

use super::{
    App, Message,
    dialog_windows::{self, DialogWindow, DialogWindows},
    drive_bar::{self, DriveBarLayout},
    window_state::{self, Geometry, WindowState},
};
use crate::{
    i18n::{error_text, t},
    mcp::{self, McpServer, McpStatus},
    theme::{self, ThemeChoice},
};

mod exit;

pub(super) use exit::leave;

/// What the window starts with; cloned into iced's start function.
#[derive(Clone)]
struct Setup {
    engine: Engine,
    db: Option<Db>,
    geometry: Geometry,
    drive_layout: DriveBarLayout,
    mcp: McpStatus,
    server: McpServer,
    theme: ThemeChoice,
    /// The keymap in force: the standard with the person's deviations (RC-100-21).
    keymap: Keymap,
    notice: Option<String>,
    /// The kept masks of Num+ and Num-.
    masks: Vec<String>,
    info: Option<String>,
    /// Where each dialog window was left.
    dialogs: Vec<(DialogWindow, Geometry)>,
}

/// Opens the window and runs until it closes.
///
/// # Errors
///
/// No runtime starts, no start directory can be listed, or the window cannot open.
pub fn run() -> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Runtime::new()?;
    // The copies of files opened from archives by runs that have ended (RC-030-02).
    runtime.spawn_blocking(rc_engine::unpacked::remove_stale);
    let setup = runtime.block_on(prepare())?;
    let server = Arc::clone(&setup.server);
    exit::prepare(runtime.handle().clone(), Arc::clone(&server));
    let shown = iced::daemon(
        move || App::boot(setup.clone()),
        App::update,
        App::window_root,
    )
    .title(App::title)
    .theme(App::theme)
    .subscription(App::subscription)
    .default_font(theme::UI_FONT)
    // The buttons' icons (RC-080-07), compiled in.
    .font(theme::icon::FONT_BYTES)
    .run();
    tracing::debug!("iced ended");
    if !exit::claim() {
        exit::wait_for_guard();
    }
    exit::finish(runtime.handle(), &server);
    tracing::debug!(limit = ?exit::RUNTIME_LIMIT, "the app's runtime stops");
    runtime.shutdown_timeout(exit::RUNTIME_LIMIT);
    shown?;
    tracing::debug!("the process ends");
    // Nothing is left to store; no drop may hold the process (RC-080-58).
    std::process::exit(0)
}

/// The database, the engine and the MCP server, before the window opens.
async fn prepare() -> Result<Setup, EngineError> {
    let mut notice = None;
    let mut info = None;
    // A restore staged in the last run is swapped in before anything opens the file - only the
    // one whose digest the window's restore stored in the database and that passes its check
    // again (RC-080-09); another file
    // of that name is set aside, and the notice says so.
    if let Ok(path) = Db::default_path() {
        match rc_db::apply_pending_restore(&path).await {
            Ok(true) => info = Some(t!("db.restore_applied").to_owned()),
            Ok(false) => {}
            Err(error) => notice = Some(error_text(error.code())),
        }
    }
    let db = match open_db().await {
        Ok(db) => Some(db),
        Err(error) => {
            notice = Some(t!("status.db_unavailable", error = error));
            // A private database keeps the MCP server running; nothing of it is kept.
            Db::open_in_memory().await.ok()
        }
    };
    if let Some(db) = &db {
        // Warn and error lines into the error log, the console on stderr; the watch for a lost
        // graphics device beside them.
        rc_db::install_diagnostics_with(db, crate::gpu_loss::layer());
    }
    // The accent colour, found off the UI thread.
    theme::init();
    let home = start_directory();
    let left = VfsPath::local(start_folder(db.as_ref(), LEFT_START_KEY, &home).await);
    let right = VfsPath::local(start_folder(db.as_ref(), RIGHT_START_KEY, &home).await);
    let engine = match Engine::open(left, right).await {
        Ok(engine) => engine,
        Err(error) => {
            tracing::warn!(
                code = error.code(),
                "the start folders do not open; both panels start at the root"
            );
            Engine::headless(VfsPath::local(root_directory())).await?
        }
    };
    // The shells a terminal can run - local ones and one per WSL distribution - found off the
    // UI thread, for the picker.
    match engine.dispatch(Command::ListTerminals {}).await {
        Ok(Outcome::Terminals { shells, .. }) => crate::term::set_shells(shells),
        Ok(_) => {}
        Err(error) => tracing::warn!(code = error.code(), "the terminal shells were not found"),
    }
    if let Some(db) = &db {
        engine.attach_db(db.clone());
        // The hidden-files switch of the last run.
        engine.restore_settings().await;
        // The tabs of the last run (RC-010-03), unless a start folder is forced.
        if std::env::var_os("RC_START_DIR").is_none() {
            match engine.restore_tabs().await {
                Ok(relocated) => info = info.or(super::tabs::relocated_note(&relocated)),
                Err(error) => {
                    tracing::warn!(code = error.code(), "the last run's tabs were not restored");
                }
            }
        }
        // The transfers the last run left in the middle, back in the queue, paused (RC-060-07).
        match engine.restore_transfers().await {
            Ok(restored) if !restored.is_empty() => {
                info = info.or_else(|| Some(t!("queue.restored", count = restored.len())));
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(
                    code = error.code(),
                    "the last run's transfers were not restored"
                );
            }
        }
        // The saved transfer lists marked "restore at start", loaded and stopped (RC-010-15).
        match engine.restore_queue_lists().await {
            Ok(loaded) if !loaded.is_empty() => {
                info = info.or_else(|| Some(t!("queue.lists_restored", count = loaded.len())));
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(code = error.code(), "the queue lists were not restored"),
        }
        // The installed plugins (RC-100-09): beside the database, the bundled ones beside the
        // executable; an update installed last run takes over now.
        if let Some(data_dir) = db.path().and_then(Path::parent) {
            engine.attach_plugins(data_dir.to_path_buf(), rc_engine::bundled_plugins_dir());
            if let Err(error) = engine.start_plugins().await {
                tracing::warn!(
                    code = error.code(),
                    "the installed plugins were not checked"
                );
            }
        }
        match Vault::open(db.clone()).await {
            Ok(vault) => {
                engine.attach_vault(vault);
            }
            // The app runs without its vault; stored secrets stay out of reach until a restart.
            Err(error) => notice = notice.or_else(|| Some(error_text(error.code()))),
        }
    }
    let geometry = match &db {
        Some(db) => window_state::load(db).await,
        None => None,
    };
    let masks = match &db {
        Some(db) => super::mru::MASKS.load(db).await,
        None => Vec::new(),
    };
    let drive_layout = match &db {
        Some(db) => drive_bar::load_layout(db).await,
        None => DriveBarLayout::default(),
    };
    let dialogs = match &db {
        Some(db) => dialog_windows::load(db).await,
        None => Vec::new(),
    };
    // The window answers the MCP calls that ask; from before the server starts (RC-010-13).
    engine.attach_window();
    let (server, mcp) = match &db {
        Some(db) => {
            let instance_file = rc_mcp::instance::default_path().ok();
            mcp::start(engine.clone(), db.clone(), instance_file).await
        }
        None => (None, McpStatus::Off),
    };
    let theme = match &db {
        Some(db) => saved_theme(db).await,
        None => ThemeChoice::default(),
    };
    let keymap = engine.keymap().await;
    Ok(Setup {
        keymap,
        engine,
        db,
        geometry: WindowState::new(geometry).geometry(),
        drive_layout,
        mcp,
        server: Arc::new(Mutex::new(server)),
        theme,
        notice,
        masks,
        info,
        dialogs,
    })
}

async fn saved_theme(db: &Db) -> ThemeChoice {
    let value = stored_setting(db, THEME_KEY).await;
    ThemeChoice::from_setting(value.as_ref().and_then(|v| v.as_str()).unwrap_or_default())
}

/// Where a panel starts: `RC_START_DIR` when set, else its setting when that names a folder,
/// else `home`.
async fn start_folder(db: Option<&Db>, key: &str, home: &Path) -> PathBuf {
    if std::env::var_os("RC_START_DIR").is_some() {
        return home.to_path_buf();
    }
    let stored = match db {
        Some(db) => stored_setting(db, key).await,
        None => None,
    };
    let folder = stored
        .as_ref()
        .and_then(|value| value.as_str())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    match folder {
        Some(folder) if tokio::fs::metadata(&folder).await.is_ok_and(|m| m.is_dir()) => folder,
        _ => home.to_path_buf(),
    }
}

async fn open_db() -> rc_db::Result<Db> {
    Db::open(Db::default_path()?).await
}

/// Where both panels start: `RC_START_DIR` when set, else the home directory.
fn start_directory() -> PathBuf {
    std::env::var_os("RC_START_DIR")
        .map(PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf()))
        .unwrap_or_else(root_directory)
}

/// The start when the home directory cannot be listed.
fn root_directory() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("C:\\")
    } else {
        PathBuf::from("/")
    }
}

impl App {
    fn boot(setup: Setup) -> (Self, Task<Message>) {
        let mut window = setup.geometry.window_settings();
        // The title bar's, Alt+Tab's and the taskbar's icon (RC-010-19).
        window.icon = super::settings::window_icon();
        // Its X asks first: the tabs and the places are stored before the app ends.
        window.exit_on_close_request = false;
        let (main, opened) = window::open(window);
        let mut app = Self::new(setup.engine, setup.keymap);
        app.dialog_windows = DialogWindows::new(main, setup.dialogs);
        app.db = setup.db;
        app.window = WindowState::new(Some(setup.geometry));
        app.mcp = setup.mcp;
        app.mcp_server = setup.server;
        app.notice = setup.notice;
        app.mask_history = setup.masks;
        app.drive_bar.layout = setup.drive_layout;
        app.info = setup.info;
        app.mcp_questions = app.engine.mcp_questions();
        app.mcp_grants = app.engine.mcp_grants();
        // `RC_THEME=dark|light` overrides the setting (screenshots of both looks).
        let choice = match std::env::var("RC_THEME").as_deref() {
            Ok("dark") => ThemeChoice::Dark,
            Ok("light") => ThemeChoice::Light,
            _ => setup.theme,
        };
        // The system's mode is asked for in any case: the dialog can switch to it later.
        let _ = app.set_theme(choice);
        let icons = app.load_icon_mode();
        let fkey_bar = app.file_ops_settings_task();
        let stripes = app.stripes_task();
        // What F10 does (RC-080-17).
        let f10 = app.f10_task();
        // Whether the queue opens in a window of its own (RC-010-14).
        let queue = app.queue_own_task();
        // Whether the large dialogs open in windows of their own (RC-080-15).
        let dialogs = app.dialog_windows_task();
        // The column widths and the panel divider (RC-010-20).
        let widths = app.resize_task();
        // Whether a plain click clears the marks (RC-010-02).
        let clicks = app.click_clears_task();
        // Whether Enter checks a checksum file (RC-020-10).
        let checksum_enter = app.checksum_enter_task();
        // Whether the dialogs' access keys are always underlined (RC-080-16).
        let access_keys = app.access_keys_task();
        // What Ctrl+wheel, Ctrl++ and Ctrl+- do in the viewer (RC-020-12).
        let text_zoom = app.text_zoom_task();
        // Whether a plugin document's text shows its colours (RC-100-10).
        let token_colors = app.token_colors_task();
        // Whether tabs are dragged and the menus show their keys (RC-010-24).
        let tabs = app.tabs_settings_task();
        // How the terminals copy their selection (RC-040-07).
        let terminal_copy = app.terminal_copy_task();
        // The quick search's settings and the command line's history (RC-010-08).
        let cmdline = super::cmdline::load(app.db.clone());
        // Whether the menu bar shows, read once the main window is open: the bar is attached to
        // it then (RC-080-18).
        let opened = opened.discard().chain(app.menu_bar_task());
        (
            app,
            Task::batch([
                opened,
                system::theme().map(Message::SystemTheme),
                icons,
                fkey_bar,
                stripes,
                f10,
                queue,
                dialogs,
                widths,
                clicks,
                checksum_enter,
                access_keys,
                text_zoom,
                token_colors,
                tabs,
                terminal_copy,
                cmdline,
            ]),
        )
    }
}

// The website's and the wiki's screenshots start the window through `App::boot` with a setup of
// their own (RC-100-45).
#[path = "../screenshots/mod.rs"]
pub(crate) mod screenshots;
