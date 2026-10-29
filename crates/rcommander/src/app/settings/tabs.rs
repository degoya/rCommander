//! The tabs of the settings' Plugins, Appearance and Keyboard pages (RC-100-20), drawn with
//! `theme::page_tabs`. The Plugins page shows its installed plugins under one tab and its schema
//! rows (`plugins.*`) under the other; the Appearance page groups its schema rows by what they
//! set ([`APPEARANCE`]); the Keyboard page shows its schema rows under one tab and the table of
//! keys under the other (RC-100-21, `shortcuts.rs`). The tab each page shows is kept for the session - across pages and after
//! the window closed - and never stored, so it is the app's ([`Tabs`]), not the dialog's. The
//! menu's "Plugins..." opens its page on the first tab, a package's preview turns to it.

use iced::Element;
use rc_engine::settings::{SettingKind, SettingSpec};

use super::{Page, SettingsMessage, wrap};
use crate::{
    app::Message,
    i18n::t,
    theme::{Tokens, page_tabs},
};

/// A tab of a settings page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// Plugins: install with the preview, the table, the selected plugin's details.
    Installed,
    /// Plugins: the page's schema rows, `plugins.*`.
    PluginSettings,
    /// Appearance: the look and the parts of the window.
    Window,
    /// Appearance: the fonts of the lists, the terminal and the viewer.
    Fonts,
    /// Appearance: the file lists, their columns, the divider and the drive line.
    Lists,
    /// Appearance: the Lister's own rows.
    Viewer,
    /// Appearance: the lists' colours per look and the colours by file type (RC-100-43).
    Colours,
    /// Keyboard: the page's schema rows - F10, the access keys, the menus' keys.
    Keys,
    /// Keyboard: every key in a table, to change (RC-100-21).
    Shortcuts,
}

const PLUGINS: [Tab; 2] = [Tab::Installed, Tab::PluginSettings];
const APPEARANCE_TABS: [Tab; 5] = [
    Tab::Window,
    Tab::Fonts,
    Tab::Colours,
    Tab::Lists,
    Tab::Viewer,
];
const KEYBOARD: [Tab; 2] = [Tab::Keys, Tab::Shortcuts];

/// The Appearance page's schema rows by tab; every row of the page is in exactly one
/// (`tabs_tests.rs` holds them to the schema).
pub const APPEARANCE: &[(Tab, &[&str])] = &[
    (
        Tab::Window,
        &[
            "appearance.theme",
            "appearance.menu_bar",
            "appearance.function_key_bar",
            "appearance.dialog_windows",
            "appearance.gpu_loss_notice",
        ],
    ),
    (
        Tab::Fonts,
        &[
            "appearance.list_font",
            "appearance.list_font_size",
            "terminal.font",
            "terminal.font_size",
            "viewer.font",
            "viewer.font_size",
        ],
    ),
    (
        Tab::Colours,
        &[
            "appearance.colours.light.text",
            "appearance.colours.light.background",
            "appearance.colours.light.stripe",
            "appearance.colours.light.cursor",
            "appearance.colours.light.cursor_inactive",
            "appearance.colours.light.cursor_text",
            "appearance.colours.light.marked",
            "appearance.colours.light.accent",
            "appearance.colours.dark.text",
            "appearance.colours.dark.background",
            "appearance.colours.dark.stripe",
            "appearance.colours.dark.cursor",
            "appearance.colours.dark.cursor_inactive",
            "appearance.colours.dark.cursor_text",
            "appearance.colours.dark.marked",
            "appearance.colours.dark.accent",
            "appearance.colour_rules",
        ],
    ),
    (
        Tab::Lists,
        &[
            "appearance.alternate_rows",
            "appearance.file_icons",
            "appearance.name_cut",
            "appearance.name_tip_delay",
            "appearance.mark_hidden",
            "columns.ext_width",
            "columns.size_width",
            "columns.packed_width",
            "columns.date_width",
            "columns.rights_width",
            "columns.owner_width",
            "columns.owner",
            "panels.split_permille",
            "panels.drive_line_volume",
            "panels.drive_line_buttons",
        ],
    ),
    (
        Tab::Viewer,
        &[
            "viewer.markdown_preview",
            "viewer.picture_megapixels",
            "viewer.dos_extensions",
            "viewer.dos_tight_lines",
            "viewer.text_zoom",
            "viewer.text_zoom_keep",
        ],
    ),
];

/// The tabs of `page`, in their order; none for a page without.
pub fn of(page: Page) -> &'static [Tab] {
    match page {
        Page::Plugins => &PLUGINS,
        Page::Appearance => &APPEARANCE_TABS,
        Page::Keyboard => &KEYBOARD,
        _ => &[],
    }
}

fn title(tab: Tab) -> &'static str {
    match tab {
        Tab::Installed => t!("settings.tab.installed"),
        Tab::PluginSettings => t!("settings.tab.plugin_settings"),
        Tab::Window => t!("settings.tab.window"),
        Tab::Fonts => t!("settings.tab.fonts"),
        Tab::Lists => t!("settings.tab.lists"),
        Tab::Viewer => t!("settings.tab.viewer"),
        Tab::Colours => t!("settings.tab.colours"),
        Tab::Keys => t!("settings.tab.keys"),
        Tab::Shortcuts => t!("settings.tab.shortcuts"),
    }
}

/// The tab each page with tabs shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tabs {
    plugins: Tab,
    appearance: Tab,
    keyboard: Tab,
}

impl Default for Tabs {
    fn default() -> Self {
        Self {
            plugins: Tab::Installed,
            appearance: Tab::Window,
            keyboard: Tab::Keys,
        }
    }
}

impl Tabs {
    /// The tab `page` shows; none for a page without tabs.
    pub fn chosen(&self, page: Page) -> Option<Tab> {
        match page {
            Page::Plugins => Some(self.plugins),
            Page::Appearance => Some(self.appearance),
            Page::Keyboard => Some(self.keyboard),
            _ => None,
        }
    }

    /// `tab` shown on its page.
    pub fn choose(&mut self, tab: Tab) {
        if PLUGINS.contains(&tab) {
            self.plugins = tab;
        } else if KEYBOARD.contains(&tab) {
            self.keyboard = tab;
        } else {
            self.appearance = tab;
        }
    }

    /// `page` opened from a menu: on its first tab.
    pub fn first(&mut self, page: Page) {
        if let Some(first) = of(page).first() {
            self.choose(*first);
        }
    }

    /// Ctrl+Tab (`back`: Ctrl+Shift+Tab) on `page`: its next tab, round; nothing on a page
    /// without tabs.
    pub fn turn(&mut self, page: Page, back: bool) {
        if let Some(chosen) = self.chosen(page) {
            self.choose(page_tabs::turn(of(page), chosen, back));
        }
    }
}

/// Whether the schema row `spec` of a page is drawn under its tab `tab`; on a page without tabs
/// (`None`), every row. The keymap's deviations are the Shortcuts tab's table, never a row.
pub fn shows(tab: Option<Tab>, spec: &SettingSpec) -> bool {
    if matches!(spec.kind, SettingKind::Keymap { .. }) {
        return false;
    }
    match tab {
        None | Some(Tab::PluginSettings | Tab::Keys) => true,
        Some(Tab::Installed | Tab::Shortcuts) => false,
        Some(tab) => appearance_tab(spec.key) == tab,
    }
}

/// The Appearance tab of the row `key`; a row no group names stands on the first, so none is
/// ever lost.
pub fn appearance_tab(key: &str) -> Tab {
    APPEARANCE
        .iter()
        .find(|(_, keys)| keys.contains(&key))
        .map_or(Tab::Window, |(tab, _)| *tab)
}

/// The tabs of `page` over its content, `chosen` marked.
pub fn strip<'a>(page: Page, chosen: Tab, tokens: &'static Tokens) -> Element<'a, Message> {
    let tabs: Vec<(Tab, &'static str)> = of(page).iter().map(|tab| (*tab, title(*tab))).collect();
    page_tabs::strip(
        &tabs,
        chosen,
        "settings.tabs",
        |tab| wrap(SettingsMessage::Tab(tab)),
        tokens,
    )
}

#[cfg(test)]
#[path = "tabs_tests.rs"]
mod tests;
