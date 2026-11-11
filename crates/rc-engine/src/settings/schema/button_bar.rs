//! The button bars and the other bars of the window (RC-100-87): the button bar above the panels
//! (on, owner 2026-11-07) and the vertical one between them (off), the bar each shows by its id,
//! the size of their symbols, and - Total Commander's Configuration > Layout - the drive bar, the
//! command line and the status bar, each shown or hidden. None is protected; `view.toggle_bar`
//! switches the bars as the View menu does.
//!
//! The buttons' look (RC-110-15), as Total Commander's Change button bar dialog sets it for both
//! bars: the symbol's size as a number (TC's "Size" field), the room around the symbol (TC keeps
//! the button's size apart from the symbol's, "Small icons"), and flat buttons without a frame
//! ("Flat icons"). TC has no space between buttons, so neither has rCommander.

use super::{SettingKind, SettingPage, SettingSpec};

/// Whether the button bar shows above the panels.
pub const BUTTON_BAR_KEY: &str = "appearance.button_bar";
/// Whether the vertical button bar shows between the panels (Total Commander's "Button Bar 2").
pub const VERTICAL_BUTTON_BAR_KEY: &str = "appearance.vertical_button_bar";
/// The bar the button bar shows, by its id; 0 for the first bar.
pub const HORIZONTAL_BAR_KEY: &str = "buttonbar.horizontal";
/// The bar the vertical button bar shows, by its id; 0 for the first bar.
pub const VERTICAL_BAR_KEY: &str = "buttonbar.vertical";
/// The size of the buttons' symbols, in pixels: 16 to 48 (until 0.21 a choice of 16, 24 and 32,
/// stored as text, which reads as the number).
pub const ICON_SIZE_KEY: &str = "buttonbar.icon_size";
/// The room between a button's symbol and its frame, in pixels on each side (RC-110-15).
pub const BUTTON_PADDING_KEY: &str = "buttonbar.button_padding";
/// Whether the buttons are flat: no frame and no fill but under the pointer and the keyboard.
pub const FLAT_KEY: &str = "buttonbar.flat";
/// Whether the drive bar shows.
pub const DRIVE_BAR_KEY: &str = "appearance.drive_bar";
/// Whether the command line shows under the panels.
pub const COMMAND_LINE_KEY: &str = "appearance.command_line";
/// Whether the status bar shows at the bottom of the window.
pub const STATUS_BAR_KEY: &str = "appearance.status_bar";

/// The largest id a bar setting takes.
const MAX_BAR_ID: i64 = 2_147_483_647;

const fn switch(
    key: &'static str,
    default: bool,
    label: &'static str,
    help: &'static str,
) -> SettingSpec {
    SettingSpec {
        key,
        page: SettingPage::Appearance,
        kind: SettingKind::Bool { default },
        label,
        help,
        protected: false,
        restart: false,
    }
}

const fn bar(key: &'static str, label: &'static str, help: &'static str) -> SettingSpec {
    SettingSpec {
        key,
        page: SettingPage::Appearance,
        kind: SettingKind::Int {
            min: 0,
            max: MAX_BAR_ID,
            default: 0,
        },
        label,
        help,
        protected: false,
        restart: false,
    }
}

/// On by default with the bar "Default" (owner 2026-11-07).
pub(super) const BUTTON_BAR: SettingSpec = switch(
    BUTTON_BAR_KEY,
    true,
    "setting.appearance.button_bar",
    "setting.appearance.button_bar.help",
);
/// Off by default: the divider between the panels stays as it was.
pub(super) const VERTICAL_BUTTON_BAR: SettingSpec = switch(
    VERTICAL_BUTTON_BAR_KEY,
    false,
    "setting.appearance.vertical_button_bar",
    "setting.appearance.vertical_button_bar.help",
);
pub(super) const HORIZONTAL_BAR: SettingSpec = bar(
    HORIZONTAL_BAR_KEY,
    "setting.buttonbar.horizontal",
    "setting.buttonbar.horizontal.help",
);
pub(super) const VERTICAL_BAR: SettingSpec = bar(
    VERTICAL_BAR_KEY,
    "setting.buttonbar.vertical",
    "setting.buttonbar.vertical.help",
);

/// The smallest symbol.
pub const MIN_ICON_SIZE: i64 = 16;
/// The largest symbol: Total Commander's own symbols come in 16, 24, 32 and 48.
pub const MAX_ICON_SIZE: i64 = 48;
/// The most room around a symbol.
pub const MAX_BUTTON_PADDING: i64 = 12;
/// The room around a symbol until 0.21, which stays the default.
pub const DEFAULT_BUTTON_PADDING: i64 = 5;

pub(super) const ICON_SIZE: SettingSpec = SettingSpec {
    key: ICON_SIZE_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Int {
        min: MIN_ICON_SIZE,
        max: MAX_ICON_SIZE,
        default: 24,
    },
    label: "setting.buttonbar.icon_size",
    help: "setting.buttonbar.icon_size.help",
    protected: false,
    restart: false,
};
pub(super) const BUTTON_PADDING: SettingSpec = SettingSpec {
    key: BUTTON_PADDING_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Int {
        min: 0,
        max: MAX_BUTTON_PADDING,
        default: DEFAULT_BUTTON_PADDING,
    },
    label: "setting.buttonbar.button_padding",
    help: "setting.buttonbar.button_padding.help",
    protected: false,
    restart: false,
};
/// Off by default: the buttons keep their frame, as until 0.21.
pub(super) const FLAT: SettingSpec = switch(
    FLAT_KEY,
    false,
    "setting.buttonbar.flat",
    "setting.buttonbar.flat.help",
);
pub(super) const DRIVE_BAR: SettingSpec = switch(
    DRIVE_BAR_KEY,
    true,
    "setting.appearance.drive_bar",
    "setting.appearance.drive_bar.help",
);
pub(super) const COMMAND_LINE: SettingSpec = switch(
    COMMAND_LINE_KEY,
    true,
    "setting.appearance.command_line",
    "setting.appearance.command_line.help",
);
pub(super) const STATUS_BAR: SettingSpec = switch(
    STATUS_BAR_KEY,
    true,
    "setting.appearance.status_bar",
    "setting.appearance.status_bar.help",
);
