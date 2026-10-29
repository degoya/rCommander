//! The file lists' column widths and the divider between the panels (RC-010-20): dragged in the
//! window, stored here, shared by both panels. A width is in pixels at the default list font
//! size; the window scales it with the chosen size (RC-010-17). The owner column is switched on
//! here too (RC-060-02), and the menu bar off (RC-080-18). None is protected.

use super::{SettingKind, SettingPage, SettingSpec};

/// The extension column's width.
pub const EXT_WIDTH_KEY: &str = "columns.ext_width";
/// The size column's width.
pub const SIZE_WIDTH_KEY: &str = "columns.size_width";
/// The packed size column's width, inside an archive.
pub const PACKED_WIDTH_KEY: &str = "columns.packed_width";
/// The date column's width.
pub const DATE_WIDTH_KEY: &str = "columns.date_width";
/// The rights column's width, on a server.
pub const RIGHTS_WIDTH_KEY: &str = "columns.rights_width";
/// The owner column's width (RC-060-02).
pub const OWNER_WIDTH_KEY: &str = "columns.owner_width";
/// Whether the file lists show the owner column where the backend names owners (RC-060-02).
pub const OWNER_COLUMN_KEY: &str = "columns.owner";
/// The left panel's share of the panels' width, in per mille.
pub const PANEL_SPLIT_KEY: &str = "panels.split_permille";
/// Whether the menu bar shows at the window's top (RC-080-18).
pub const MENU_BAR_KEY: &str = "appearance.menu_bar";

/// The widest a fixed column gets.
const MAX_WIDTH: i64 = 640;

const fn width(
    key: &'static str,
    min: i64,
    default: i64,
    label: &'static str,
    help: &'static str,
) -> SettingSpec {
    SettingSpec {
        key,
        page: SettingPage::Appearance,
        kind: SettingKind::Int {
            min,
            max: MAX_WIDTH,
            default,
        },
        label,
        help,
        protected: false,
        restart: false,
    }
}

pub(super) const EXT_WIDTH: SettingSpec = width(
    EXT_WIDTH_KEY,
    24,
    56,
    "setting.columns.ext_width",
    "setting.columns.ext_width.help",
);
pub(super) const SIZE_WIDTH: SettingSpec = width(
    SIZE_WIDTH_KEY,
    40,
    96,
    "setting.columns.size_width",
    "setting.columns.size_width.help",
);
pub(super) const PACKED_WIDTH: SettingSpec = width(
    PACKED_WIDTH_KEY,
    40,
    96,
    "setting.columns.packed_width",
    "setting.columns.packed_width.help",
);
pub(super) const DATE_WIDTH: SettingSpec = width(
    DATE_WIDTH_KEY,
    48,
    128,
    "setting.columns.date_width",
    "setting.columns.date_width.help",
);
pub(super) const RIGHTS_WIDTH: SettingSpec = width(
    RIGHTS_WIDTH_KEY,
    40,
    84,
    "setting.columns.rights_width",
    "setting.columns.rights_width.help",
);
pub(super) const OWNER_WIDTH: SettingSpec = width(
    OWNER_WIDTH_KEY,
    40,
    120,
    "setting.columns.owner_width",
    "setting.columns.owner_width.help",
);
/// Off by default, as Total Commander shows no owner until a column set asks for it.
pub(super) const OWNER_COLUMN: SettingSpec = SettingSpec {
    key: OWNER_COLUMN_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: false },
    label: "setting.columns.owner",
    help: "setting.columns.owner.help",
    protected: false,
    restart: false,
};
pub(super) const PANEL_SPLIT: SettingSpec = SettingSpec {
    key: PANEL_SPLIT_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Int {
        min: 100,
        max: 900,
        default: 500,
    },
    label: "setting.panels.split_permille",
    help: "setting.panels.split_permille.help",
    protected: false,
    restart: false,
};
/// On by default: the menu bar is the native way into every command (RC-080-18).
pub(super) const MENU_BAR: SettingSpec = SettingSpec {
    key: MENU_BAR_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: true },
    label: "setting.appearance.menu_bar",
    help: "setting.appearance.menu_bar.help",
    protected: false,
    restart: false,
};
