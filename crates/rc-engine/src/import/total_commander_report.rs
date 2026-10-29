//! What `import.total_commander` found, planned and did, and what
//! `import.total_commander_passwords` stored (RC-050-09). The report repeats names, folders, hosts,
//! users and ports - never a password or its obfuscated form, only [`TcPassword`].

use rc_db::connections::ImportStatus;
use serde::{Deserialize, Serialize};

use super::tc_password::TcPassword;
use crate::{ConnectionKind, PanelId};

/// The root folder of the hotlist and of the connection tree the import fills.
pub const DEFAULT_FOLDER: &str = "Imported from Total Commander";

/// The prefix of the `import_source` of an imported connection.
pub const SOURCE_PREFIX: &str = "total_commander:";

/// Whether imported tabs join the panel's tabs or take their place.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TcTabsMode {
    #[default]
    Append,
    Replace,
}

/// Which parts an import reads; all by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TcParts {
    pub hotlist: bool,
    pub tabs: bool,
    pub connections: bool,
    /// The colours, the rules by file type and the file lists' font (RC-050-14).
    pub look: bool,
}

impl Default for TcParts {
    fn default() -> Self {
        Self {
            hotlist: true,
            tabs: true,
            connections: true,
            look: true,
        }
    }
}

/// What happens, or happened, to a hotlist item or a tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TcRowStatus {
    /// Not there yet: added when selected, unless it is a dry run.
    New,
    Created,
    /// There already, with the same folder: nothing to do.
    Exists,
    /// There already with another folder: changed when selected.
    Update,
    Updated,
    /// A tab whose folder does not exist (now): added only when asked for by its key.
    Missing,
    /// Not importable; `reason` is a [`super::total_commander::TcSkip`] code.
    Skipped {
        reason: String,
    },
}

/// One item of Total Commander's directory menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TcHotlistRow {
    /// `menu<N>`, what `only` names.
    pub key: String,
    pub name: String,
    /// The submenus it is in, outermost first.
    pub folders: Vec<String>,
    /// A submenu.
    pub folder: bool,
    pub path: Option<String>,
    pub target: Option<String>,
    pub selected: bool,
    #[serde(flatten)]
    pub status: TcRowStatus,
}

/// One folder tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TcTabRow {
    /// `left<N>` or `right<N>`.
    pub key: String,
    pub panel: PanelId,
    pub path: Option<String>,
    pub caption: Option<String>,
    pub locked: bool,
    /// The panel's active tab in Total Commander.
    pub active: bool,
    pub selected: bool,
    #[serde(flatten)]
    pub status: TcRowStatus,
}

/// One connection of `wcx_ftp.ini`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TcConnectionRow {
    /// `ftp:<section>`.
    pub key: String,
    pub name: String,
    pub folders: Vec<String>,
    /// `None` for a protocol rCommander does not speak.
    pub kind: Option<ConnectionKind>,
    pub host: String,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub directory: Option<String>,
    /// Whether Total Commander keeps a password: only ever this, never the value. Not named
    /// `password`, which the MCP mask would blank.
    pub password_state: TcPassword,
    pub selected: bool,
    /// `refused` carries a connection-tree code, or `tc_protocol_unknown`, `tc_port_invalid`,
    /// `tc_too_deep`.
    #[serde(flatten)]
    pub status: ImportStatus,
}

/// One colour, rule by file type or font of `wincmd.ini` (RC-050-14).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TcLookRow {
    /// `colour.<light|dark>.<token>`, `rule.<N>` or `font.list`.
    pub key: String,
    /// Where Total Commander keeps it, as `[Colors] CursorColor=128`.
    pub source: String,
    /// What rCommander takes: `#800000`, a rule's `#light` or `#light / #dark`, `Consolas 13`;
    /// empty when skipped.
    pub value: String,
    pub selected: bool,
    #[serde(flatten)]
    pub status: TcRowStatus,
}

/// What `import.total_commander` read, planned and did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TcImportReport {
    pub dry_run: bool,
    /// The files read; `None` when not found or not needed.
    pub wincmd_ini: Option<String>,
    pub ftp_ini: Option<String>,
    pub tabs_mode: TcTabsMode,
    pub hotlist: Vec<TcHotlistRow>,
    pub tabs: Vec<TcTabRow>,
    pub connections: Vec<TcConnectionRow>,
    pub look: Vec<TcLookRow>,
    /// Codes of what was not read: `wincmd_not_found`, `ftp_ini_not_found`, `value_too_long`,
    /// `dir_menu_redirected`; the window's text is `tc_import.warning.<code>`.
    pub warnings: Vec<String>,
}

/// One connection whose password was not stored, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TcPasswordSkip {
    pub key: String,
    /// `master_password`, `unreadable`, `none`, `anonymous`, `not_imported`, or an error code.
    pub reason: String,
}

/// What `import.total_commander_passwords` stored: keys only, never a value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TcPasswordsReport {
    pub stored: Vec<String>,
    pub skipped: Vec<TcPasswordSkip>,
}
