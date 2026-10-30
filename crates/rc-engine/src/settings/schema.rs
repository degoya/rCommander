//! The settings schema: every setting the dialog shows and `settings.get` reads, with its type,
//! default, page and i18n keys.
//!
//! Only settings the code reads today are here. The MCP token and the per-class policy are not
//! rows: the token is a secret (shown only to copy), the policy has its own type in `rc-mcp`;
//! both are protected like every `mcp.*` key. The rows of one area are in `schema/<area>.rs`
//! (fonts, layout, keys, delete, drive_line, transfers, windows, exchange, mouse, checksum, sort,
//! viewer, mcp, plugins, colours, archive, quick_search, ftp, update, language); how a value is
//! checked against its row is `schema/check.rs`.

pub(super) mod archive;
pub(super) mod backup;
mod check;
pub(super) mod checksum;
mod choices;
mod colours;
mod delete;
mod drive_line;
pub(super) mod drives;
mod exchange;
mod fonts;
pub(super) mod ftp;
mod keys;
mod language;
pub(super) mod layout;
pub(super) mod login_scripts;
mod lookup;
mod mcp;
mod mouse;
mod plugins;
mod quick_search;
pub(super) mod sort;
mod ssh_agent;
pub(super) mod terminal;
mod transfers;
pub(super) mod update;
mod vault;
pub(super) mod viewer;
mod windows;

pub use archive::ARCHIVE_ENTER_KEY;
pub use check::parse_associations;
pub use colours::{
    COLOUR_KEYS, COLOUR_RULES_KEY, COLOUR_TOKENS, ColourRule, MAX_COLOUR_RULES, colour_key,
    colour_value, parse_colour_rules, write_colour_rules,
};
pub use delete::PERMANENT_WITHOUT_TRASH_KEY;
pub use drive_line::{DRIVE_LINE_BUTTONS_KEY, DRIVE_LINE_VOLUME_KEY};
pub use exchange::{
    EXPORT_CONNECTIONS_KEY, EXPORT_ENCRYPTED_KEY, EXPORT_HOTLIST_KEY, EXPORT_SETTINGS_KEY,
    EXPORT_TRUST_KEY,
};
pub use fonts::{
    DEFAULT_FONT_SIZE, LIST_FONT_KEY, LIST_FONT_SIZE_KEY, MARK_HIDDEN_KEY, MAX_FONT_SIZE,
    MIN_FONT_SIZE, NAME_CUT_KEY, NAME_TIP_DELAY_KEY, TERMINAL_FONT_KEY, TERMINAL_FONT_SIZE_KEY,
    VIEWER_FONT_KEY, VIEWER_FONT_SIZE_KEY,
};
pub use keys::{
    CONFIRM_QUIT_KEY, CONNECTIONS_FOCUS_LIST_KEY, CONTEXT_MENU_KEYS_KEY, F10_KEY, KEYMAP_KEY,
    UNDERLINE_ACCESS_KEYS_KEY,
};
pub use language::LANGUAGE_KEY;
pub use lookup::{is_protected, setting};
pub use mcp::{MCP_EXEC_SESSION_MINUTES_KEY, MCP_HEADLESS_CODE_KEY};
pub use mouse::{CLICK_CLEARS_MARKS_KEY, TAB_DRAG_KEY};
pub use plugins::{
    ASSOCIATIONS_KEY, CRASH_LIMIT_KEY, DEVELOPMENT_MODE_KEY, ENABLED_KEY, OUTPUT_LIMIT_KEY,
    VIEWER_TOKEN_COLORS_KEY,
};
pub use quick_search::{
    QUICK_SEARCH_ANYWHERE_KEY, QUICK_SEARCH_CASE_KEY, QUICK_SEARCH_FILTERS_KEY,
    QUICK_SEARCH_KEYS_KEY,
};
pub use ssh_agent::AGENT_MAX_KEYS_KEY;
pub use transfers::{
    HIDE_SINGLE_LANE_KEY, ONE_JOB_PER_ITEM_KEY, QUEUE_WINDOW_OWN_KEY, SHOW_AT_ONCE_KEY,
    STRIP_ROWS_KEY, TOOLS_AT_ONCE_KEY, TOTAL_SPEED_KEY, TRANSFER_BUFFER_KEY,
    TRANSFER_PER_CONNECTION_KEY, TRANSFER_RETRY_COUNT_KEY, TRANSFER_RETRY_DELAY_KEY,
    TRANSFER_TOTAL_KEY,
};
pub use update::{
    UPDATE_AUTO_KEY, UPDATE_CHANNEL_KEY, UPDATE_CHECK_KEY, UPDATE_IDLE_KEY, UPDATE_INTERVAL_KEY,
    UPDATE_PROXY_KEY, UPDATE_RETENTION_KEY, UPDATE_SOURCE_KEY, UPDATE_WHATS_NEW_KEY,
};
pub use windows::{DIALOG_WINDOWS_KEY, GPU_LOSS_NOTICE_KEY};

/// The longest text or path a setting takes.
const MAX_TEXT: usize = 4096;

/// The dialog page a setting is shown on, in the dialog's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SettingPage {
    Appearance,
    Folders,
    Terminal,
    Vault,
    Mcp,
    /// The external viewer and editor (RC-010-07).
    Programs,
    /// Copies and moves: buffer and how many run at once (RC-060-05), retries of a lost
    /// connection (RC-060-07).
    Transfers,
    /// What keys do beyond the keymap: F10 (RC-080-17); and the keymap's deviations (RC-100-21).
    Keyboard,
    /// What the export of settings and connections starts with (RC-080-19).
    Exchange,
    /// Plugins: the master switch, suspension, output limits, associations (RC-100-09).
    Plugins,
    /// The self-update: checks, channel, source, proxy, automation (RC-100-05, RC-100-07).
    Updates,
}

/// One choice of a [`SettingKind::Choice`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice {
    /// The stored value.
    pub value: &'static str,
    /// The i18n key of its label.
    pub label: &'static str,
}

/// A setting's type, with its default and bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingKind {
    Bool {
        default: bool,
    },
    /// A whole number within `min..=max`.
    Int {
        min: i64,
        max: i64,
        default: i64,
    },
    /// One of a fixed list.
    Choice {
        choices: &'static [Choice],
        default: &'static str,
    },
    /// A short identifier: letters, digits, `.`, `_` and `-`; empty for none.
    Id {
        default: &'static str,
    },
    /// An absolute path, or empty for none.
    Path {
        default: &'static str,
    },
    /// A font family's name, or empty for the area's default (RC-010-17).
    Font {
        default: &'static str,
    },
    /// File extensions without dots, separated by spaces; empty for none (RC-020-11).
    Extensions {
        default: &'static str,
    },
    /// Extensions given to a plugin: `csv=<plugin id> tsv=<plugin id>`; empty for none
    /// (RC-100-09).
    Associations {
        default: &'static str,
    },
    /// The deviations from the standard keymap: `Ctrl+K=copy F5=`; empty for none (RC-100-21,
    /// `rc_engine::Overrides`).
    Keymap {
        default: &'static str,
    },
    /// A colour `#rrggbb`, or empty for the theme's own (RC-100-43).
    Colour {
        default: &'static str,
    },
    /// The colours by file type: one rule per line, `mask<TAB>#light[<TAB>#dark]`; empty for
    /// none (RC-100-43, `schema/colours.rs`).
    ColourRules {
        default: &'static str,
    },
    /// An `https://` folder serving the update manifests; empty for the official releases
    /// (RC-100-05).
    Source {
        default: &'static str,
    },
    /// A proxy: empty for the system's, `direct`, or `http(s)://host:port` without a login.
    Proxy {
        default: &'static str,
    },
}

/// One setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingSpec {
    /// The key in the `settings` table.
    pub key: &'static str,
    pub page: SettingPage,
    pub kind: SettingKind,
    /// The i18n key of its label.
    pub label: &'static str,
    /// The i18n key of its help line.
    pub help: &'static str,
    /// Only a person in the window changes it: `settings.set` refuses it for every caller.
    pub protected: bool,
    /// It takes effect at the next start.
    pub restart: bool,
}

/// The light and dark look.
pub const THEME_KEY: &str = "appearance.theme";
/// The folder the left panel starts in; empty for the home folder.
pub const LEFT_START_KEY: &str = "folders.left_start";
/// The folder the right panel starts in; empty for the home folder.
pub const RIGHT_START_KEY: &str = "folders.right_start";
/// The shell a new terminal runs; empty for the first one found.
pub const SHELL_KEY: &str = "terminal.default_shell";

/// The file lists' icons: off, generic or native (RC-010-10).
pub const FILE_ICONS_KEY: &str = "appearance.file_icons";
/// Whether the function-key bar shows at the bottom of the window.
pub const FKEY_BAR_KEY: &str = "appearance.function_key_bar";
/// Whether F7 offers the name under the cursor as the new folder's name (owner, 2026-10-19).
pub const MKDIR_PREFILL_KEY: &str = "mkdir.prefill_cursor_name";
/// Whether every other row of the file lists and tables is tinted (RC-080-08).
pub const ALTERNATE_ROWS_KEY: &str = "appearance.alternate_rows";
/// The program F3 opens a file with; empty for the system's program.
pub const VIEWER_KEY: &str = "files.viewer";
/// The program F4 and Shift+F4 open a file with; empty for Notepad (Windows) or `$EDITOR`.
pub const EDITOR_KEY: &str = "files.editor";
/// Whether Num+, Num- and Shift+F2 select folders too, not only files (RC-020-08).
pub const SELECT_FOLDERS_KEY: &str = "selection.include_folders";
/// Whether Space moves the cursor to the next entry after flipping its selection, as Insert
/// does; off, the cursor stays on the entry (owner, 2026-10-24).
pub const SPACE_MOVES_DOWN_KEY: &str = "selection.space_moves_down";
/// Whether a panel's terminal follows the panel's folder (RC-040-04).
pub const FOLLOW_PANEL_KEY: &str = "terminal.follow_panel";
/// Whether a panel turning back to its files shows the folder its terminal reported.
pub const PANEL_FOLLOWS_KEY: &str = "terminal.panel_follows";
/// Whether a new shell starts with the app's shell integration (folder and prompt reports).
pub const INTEGRATION_KEY: &str = "terminal.shell_integration";

/// Every setting, in the dialog's order.
pub const SETTINGS: &[SettingSpec] = &[
    language::LANGUAGE,
    SettingSpec {
        key: THEME_KEY,
        page: SettingPage::Appearance,
        kind: SettingKind::Choice {
            choices: choices::THEMES,
            default: "system",
        },
        label: "setting.appearance.theme",
        help: "setting.appearance.theme.help",
        protected: false,
        restart: false,
    },
    SettingSpec {
        key: FKEY_BAR_KEY,
        page: SettingPage::Appearance,
        kind: SettingKind::Bool { default: true },
        label: "setting.appearance.function_key_bar",
        help: "setting.appearance.function_key_bar.help",
        protected: false,
        restart: false,
    },
    SettingSpec {
        key: ALTERNATE_ROWS_KEY,
        page: SettingPage::Appearance,
        kind: SettingKind::Bool { default: true },
        label: "setting.appearance.alternate_rows",
        help: "setting.appearance.alternate_rows.help",
        protected: false,
        restart: false,
    },
    layout::MENU_BAR,
    fonts::LIST_FONT,
    fonts::LIST_FONT_SIZE,
    fonts::NAME_CUT,
    fonts::NAME_TIP_DELAY,
    fonts::MARK_HIDDEN,
    fonts::TERMINAL_FONT,
    fonts::TERMINAL_FONT_SIZE,
    fonts::VIEWER_FONT,
    fonts::VIEWER_FONT_SIZE,
    viewer::MARKDOWN_PREVIEW,
    viewer::PICTURE_MEGAPIXELS,
    viewer::DOS_EXTENSIONS,
    viewer::DOS_TIGHT_LINES,
    viewer::TEXT_ZOOM,
    viewer::TEXT_ZOOM_KEEP,
    layout::EXT_WIDTH,
    layout::SIZE_WIDTH,
    layout::PACKED_WIDTH,
    layout::DATE_WIDTH,
    layout::RIGHTS_WIDTH,
    layout::OWNER_WIDTH,
    layout::OWNER_COLUMN,
    layout::PANEL_SPLIT,
    drive_line::VOLUME,
    drive_line::BUTTONS,
    SettingSpec {
        key: LEFT_START_KEY,
        page: SettingPage::Folders,
        kind: SettingKind::Path { default: "" },
        label: "setting.folders.left_start",
        help: "setting.folders.left_start.help",
        protected: false,
        restart: true,
    },
    SettingSpec {
        key: RIGHT_START_KEY,
        page: SettingPage::Folders,
        kind: SettingKind::Path { default: "" },
        label: "setting.folders.right_start",
        help: "setting.folders.right_start.help",
        protected: false,
        restart: true,
    },
    quick_search::KEYS,
    quick_search::ANYWHERE,
    quick_search::CASE_SENSITIVE,
    quick_search::FILTERS,
    archive::ENTER_OPENS,
    archive::DECODER_MEMORY,
    checksum::ENTER_VERIFIES,
    SettingSpec {
        key: SELECT_FOLDERS_KEY,
        page: SettingPage::Folders,
        kind: SettingKind::Bool { default: false },
        label: "setting.selection.include_folders",
        help: "setting.selection.include_folders.help",
        protected: false,
        restart: false,
    },
    SettingSpec {
        key: SPACE_MOVES_DOWN_KEY,
        page: SettingPage::Folders,
        kind: SettingKind::Bool { default: true },
        label: "setting.selection.space_moves_down",
        help: "setting.selection.space_moves_down.help",
        protected: false,
        restart: false,
    },
    mouse::CLICK_CLEARS_MARKS,
    mouse::TAB_DRAG,
    drives::FOLLOW_OTHER_PANEL,
    drives::FOLLOW_ON_OWN_DRIVE,
    sort::SORT_NAMES,
    sort::SORT_NUMBERS,
    SettingSpec {
        key: SHELL_KEY,
        page: SettingPage::Terminal,
        kind: SettingKind::Id { default: "" },
        label: "setting.terminal.default_shell",
        help: "setting.terminal.default_shell.help",
        protected: false,
        restart: false,
    },
    SettingSpec {
        key: FOLLOW_PANEL_KEY,
        page: SettingPage::Terminal,
        kind: SettingKind::Bool { default: true },
        label: "setting.terminal.follow_panel",
        help: "setting.terminal.follow_panel.help",
        protected: false,
        restart: false,
    },
    SettingSpec {
        key: PANEL_FOLLOWS_KEY,
        page: SettingPage::Terminal,
        kind: SettingKind::Bool { default: true },
        label: "setting.terminal.panel_follows",
        help: "setting.terminal.panel_follows.help",
        protected: false,
        restart: false,
    },
    terminal::INTEGRATION,
    login_scripts::WARN_SECRET_PROMPT,
    terminal::END_PROGRAMS,
    terminal::COPY_CTRL_C,
    terminal::COPY_RIGHT_CLICK,
    terminal::COPY_ON_SELECT,
    terminal::PASTE_CTRL_V,
    terminal::OSC52_COPY,
    terminal::SHIFT_SELECTS,
    terminal::PASTE_IMAGE,
    keys::F10,
    keys::UNDERLINE_ACCESS_KEYS,
    keys::CONTEXT_MENU_KEYS,
    keys::CONNECTIONS_FOCUS_LIST,
    keys::KEYMAP,
    vault::IDLE_LOCK,
    vault::MAX_UNLOCKED,
    vault::LOCK_ON_SESSION_LOCK,
    vault::WINCRED_SOURCE,
    vault::SHARED_CREDENTIALS,
    mcp::HTTP_ENABLED,
    mcp::HTTP_PORT,
    mcp::HEADLESS_CODE,
    mcp::EXEC_SESSION_MINUTES,
    // Protected: the program that runs on F3 and F4 is a person's choice, never an agent's.
    SettingSpec {
        key: VIEWER_KEY,
        page: SettingPage::Programs,
        kind: SettingKind::Path { default: "" },
        label: "setting.files.viewer",
        help: "setting.files.viewer.help",
        protected: true,
        restart: false,
    },
    SettingSpec {
        key: FILE_ICONS_KEY,
        page: SettingPage::Appearance,
        kind: SettingKind::Choice {
            choices: choices::FILE_ICONS,
            default: "native",
        },
        label: "setting.appearance.file_icons",
        help: "setting.appearance.file_icons.help",
        protected: false,
        restart: false,
    },
    SettingSpec {
        key: EDITOR_KEY,
        page: SettingPage::Programs,
        kind: SettingKind::Path { default: "" },
        label: "setting.files.editor",
        help: "setting.files.editor.help",
        protected: true,
        restart: false,
    },
    transfers::BUFFER,
    transfers::PER_CONNECTION,
    transfers::TOTAL,
    transfers::RETRY_COUNT,
    transfers::RETRY_DELAY,
    keys::CONFIRM_QUIT,
    delete::PERMANENT_WITHOUT_TRASH,
    SettingSpec {
        key: MKDIR_PREFILL_KEY,
        page: SettingPage::Transfers,
        kind: SettingKind::Bool { default: true },
        label: "setting.mkdir.prefill_cursor_name",
        help: "setting.mkdir.prefill_cursor_name.help",
        protected: false,
        restart: false,
    },
    transfers::ONE_JOB_PER_ITEM,
    transfers::QUEUE_WINDOW_OWN,
    transfers::STRIP_ROWS,
    transfers::SHOW_AT_ONCE,
    transfers::TOTAL_SPEED,
    transfers::HIDE_SINGLE_LANE,
    transfers::TOOLS_AT_ONCE,
    windows::DIALOG_WINDOWS,
    colours::LIGHT_TEXT,
    colours::LIGHT_BACKGROUND,
    colours::LIGHT_STRIPE,
    colours::LIGHT_CURSOR,
    colours::LIGHT_CURSOR_INACTIVE,
    colours::LIGHT_CURSOR_TEXT,
    colours::LIGHT_MARKED,
    colours::LIGHT_ACCENT,
    colours::DARK_TEXT,
    colours::DARK_BACKGROUND,
    colours::DARK_STRIPE,
    colours::DARK_CURSOR,
    colours::DARK_CURSOR_INACTIVE,
    colours::DARK_CURSOR_TEXT,
    colours::DARK_MARKED,
    colours::DARK_ACCENT,
    colours::RULES,
    windows::GPU_LOSS_NOTICE,
    exchange::EXPORT_SETTINGS,
    exchange::EXPORT_CONNECTIONS,
    exchange::EXPORT_TRUST,
    exchange::EXPORT_HOTLIST,
    exchange::EXPORT_ENCRYPTED,
    plugins::ENABLED,
    plugins::CRASH_LIMIT,
    plugins::OUTPUT_LIMIT,
    plugins::VIEWER_TOKEN_COLORS,
    plugins::ASSOCIATIONS,
    plugins::DEVELOPMENT_MODE,
    ssh_agent::AGENT_MAX_KEYS,
    backup::KEEP,
    ftp::RESOLVE_LINKS,
    update::CHECK_ENABLED,
    update::CHECK_INTERVAL,
    update::CHANNEL,
    update::SOURCE,
    update::PROXY,
    update::WHATS_NEW,
    update::AUTO_INSTALL,
    update::IDLE_MINUTES,
    update::RETENTION_DAYS,
];
