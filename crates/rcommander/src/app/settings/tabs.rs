//! The tabs of the settings' Plugins, Appearance and Keyboard pages (RC-100-20), drawn with
//! `theme::page_tabs`. The Plugins page shows its installed plugins under one tab and its schema
//! rows (`plugins.*`) under the other; the Appearance page groups its schema rows by what they
//! set ([`APPEARANCE`]); the Keyboard page shows its schema rows under one tab and the table of
//! keys under the other (RC-100-21, `shortcuts.rs`). The tab each page shows is kept for the session - across pages and after
//! the window closed - and never stored, so it is the app's ([`Tabs`]), not the dialog's. The
//! menu's "Plugins..." opens its page on the first tab, a package's preview turns to it.

use iced::Element;
use rc_engine::settings::{MENU_BAR_STYLE_KEY, SettingKind, SettingSpec, THUMBNAIL_EXPLORER_KEY};

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
    /// Appearance: the thumbnails' size, caches and sources, then the disk cache's button
    /// (RC-100-89, `thumbnails_page.rs`).
    Thumbnails,
    /// Appearance: the column sets and their automatic switch (RC-100-90); the sets drawn by
    /// `columns_page.rs`, the switch's rows below them.
    Columns,
    /// Appearance: the colour themes of both modes, the WSL distributions' themes and the
    /// colours by file type (RC-100-50, RC-100-56, RC-100-43); drawn by `themes_page.rs`, not
    /// row by row.
    Themes,
    /// Keyboard: the page's schema rows - F10, the access keys, the menus' keys.
    Keys,
    /// Keyboard: every key in a table, to change (RC-100-21).
    Shortcuts,
}

const PLUGINS: [Tab; 2] = [Tab::Installed, Tab::PluginSettings];
const APPEARANCE_TABS: [Tab; 7] = [
    Tab::Window,
    Tab::Fonts,
    Tab::Themes,
    Tab::Lists,
    Tab::Columns,
    Tab::Viewer,
    Tab::Thumbnails,
];
const KEYBOARD: [Tab; 2] = [Tab::Keys, Tab::Shortcuts];

/// The Appearance page's schema rows by tab; every row of the page is in exactly one
/// (`tabs_tests.rs` holds them to the schema).
pub const APPEARANCE: &[(Tab, &[&str])] = &[
    (
        Tab::Window,
        &[
            "appearance.language",
            "appearance.theme",
            "appearance.connection_themes",
            "appearance.menu_bar",
            "appearance.menu_bar_style",
            "appearance.macos_look",
            "appearance.function_key_bar",
            "appearance.button_bar",
            "appearance.vertical_button_bar",
            "buttonbar.horizontal",
            "buttonbar.vertical",
            "buttonbar.icon_size",
            "buttonbar.button_padding",
            "buttonbar.flat",
            "appearance.drive_bar",
            "appearance.command_line",
            "appearance.status_bar",
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
        Tab::Themes,
        &[
            "appearance.theme_light",
            "appearance.theme_dark",
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
            "appearance.wsl_themes",
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
            "panels.separate_tree",
            "panels.tree_width",
            "panels.drive_line_volume",
            "panels.drive_line_buttons",
        ],
    ),
    (
        Tab::Columns,
        &["columns.auto_switch", "columns.auto_default"],
    ),
    (
        Tab::Viewer,
        &[
            "viewer.markdown_preview",
            "viewer.markdown_select",
            "viewer.markdown_code_highlight",
            "viewer.picture_megapixels",
            "viewer.dos_extensions",
            "viewer.dos_tight_lines",
            "viewer.text_zoom",
            "viewer.text_zoom_keep",
            "viewer.exif",
            "viewer.step_through_selection",
            "viewer.certificates",
            "viewer.cert_expiry_warning_days",
        ],
    ),
    (
        Tab::Thumbnails,
        &[
            "thumbnails.size",
            "thumbnails.disk_cache",
            "thumbnails.cache_mb",
            "thumbnails.remote",
            "thumbnails.explorer",
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
        Tab::Thumbnails => t!("settings.tab.thumbnails"),
        Tab::Columns => t!("settings.tab.columns"),
        Tab::Themes => t!("settings.tab.themes"),
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

/// Whether the schema row `key` is drawn on this system: the menu bar's style (RC-100-66) and
/// Explorer's thumbnails (RC-100-89) only on Windows, the one system where they choose anything.
pub fn here(key: &str) -> bool {
    here_on(key, cfg!(windows))
}

fn here_on(key: &str, windows: bool) -> bool {
    ![MENU_BAR_STYLE_KEY, THUMBNAIL_EXPLORER_KEY].contains(&key) || windows
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
