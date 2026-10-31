//! What the window applies at once when a setting changes - the theme, the stripes, the fonts,
//! the colour themes and the colours by file type (RC-100-50, RC-100-43),
//! the menu bar, the drive lines, the keys (RC-100-21) and whether Cmd is Ctrl on macOS
//! (RC-100-49) - and the light or dark look. Split from `settings.rs` to keep it
//! under 500 lines (RC-100-09).

use iced::{Task, system, theme::Mode as SystemMode};
use rc_engine::Keymap;
use rc_engine::settings::{
    ALTERNATE_ROWS_KEY, DOS_TIGHT_LINES_KEY, KEYMAP_KEY, LIST_FONT_KEY, LIST_FONT_SIZE_KEY,
    MACOS_CMD_AS_CTRL_KEY, MARK_HIDDEN_KEY, MENU_BAR_KEY, NAME_CUT_KEY, NAME_TIP_DELAY_KEY,
    TERMINAL_FONT_KEY, TERMINAL_FONT_SIZE_KEY, THEME_KEY, VIEWER_FONT_KEY, VIEWER_FONT_SIZE_KEY,
    stored_bool,
};

use super::{SettingsMessage, save, themes_page, wrap};
use crate::{
    app::{App, Effect, Message, drive_bar},
    theme::ThemeChoice,
};

impl App {
    /// The engine stored a setting: the window applies what it shows at once.
    pub(in crate::app) fn setting_changed(&mut self, key: &str) -> Vec<Effect> {
        if key == ALTERNATE_ROWS_KEY {
            return vec![Effect::Task(self.stripes_task())];
        }
        if FONT_KEYS.contains(&key) {
            return vec![Effect::Task(self.fonts_task())];
        }
        if themes_page::concerns(key) {
            return vec![Effect::Task(self.themes_task())];
        }
        if key == MENU_BAR_KEY {
            return vec![Effect::Task(self.menu_bar_task())];
        }
        if key == KEYMAP_KEY || key == MACOS_CMD_AS_CTRL_KEY {
            return vec![Effect::Task(self.keymap_task())];
        }
        if drive_bar::line_concerns(key) {
            return vec![Effect::Task(self.drive_line_task())];
        }
        if crate::app::checksum::enter_concerns(key) {
            return vec![Effect::Task(self.checksum_enter_task())];
        }
        if crate::app::terminal_copy::concerns(key) {
            return vec![Effect::Task(self.terminal_copy_task())];
        }
        if key == rc_engine::settings::WARN_SECRET_PROMPT_KEY {
            return vec![Effect::Task(self.login_scripts_warn_task())];
        }
        let (true, Some(db)) = (key == THEME_KEY, self.db.clone()) else {
            return self.tabs_setting_changed(key);
        };
        let task = Task::perform(save::theme(db), |choice| {
            wrap(SettingsMessage::Theme(choice))
        });
        vec![Effect::Task(task)]
    }

    /// Reads whether the lists and tables are striped, the fonts and the themes they are drawn in
    /// and what the drive lines show (the start reads all four here).
    pub(in crate::app) fn stripes_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        let stripes = Task::perform(save::alternate_rows(db), |on| {
            wrap(SettingsMessage::Stripes(on))
        });
        Task::batch([
            stripes,
            self.fonts_task(),
            self.themes_task(),
            self.drive_line_task(),
        ])
    }

    /// Reads the chosen fonts and resolves them against the installed ones (RC-010-17).
    fn fonts_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(save::fonts(db), |fonts| {
            wrap(SettingsMessage::Fonts(Box::new(fonts)))
        })
    }

    /// Reads the keymap in force: the standard with the stored deviations (RC-100-21); and
    /// whether Cmd is Ctrl on macOS (RC-100-49), taken before the keymap comes back, so the
    /// keys and their labels change together.
    fn keymap_task(&self) -> Task<Message> {
        let engine = self.engine.clone();
        let db = self.db.clone();
        let read = async move {
            if let Some(db) = db {
                crate::keymap::set_cmd_as_ctrl(stored_bool(&db, MACOS_CMD_AS_CTRL_KEY).await);
            }
            engine.keymap().await
        };
        Task::perform(read, |keymap| {
            wrap(SettingsMessage::Keymap(Box::new(keymap)))
        })
    }

    /// The keymap in force, read: the keys run it from now, and the palette, the context menus
    /// and the function-key bar read it; the menu bar's entries show its keys.
    pub(in crate::app) fn keymap_read(&mut self, keymap: Keymap) -> Vec<Effect> {
        self.keymap = keymap;
        vec![Effect::Task(self.relabel_menu_bar())]
    }

    /// Light, dark or the system's look; the system's is asked for again, as it may have changed
    /// while it was not followed.
    pub(in crate::app) fn set_theme(&mut self, choice: ThemeChoice) -> Vec<Effect> {
        self.follow_system = choice == ThemeChoice::System;
        self.dark = choice.is_dark(self.system_dark);
        // The terminals take the colours of the mode's theme (RC-100-50).
        self.restyle_terminals();
        if self.follow_system {
            return vec![Effect::Task(system::theme().map(Message::SystemTheme))];
        }
        Vec::new()
    }

    /// The system's light or dark mode, reported at the start and on every change.
    pub(in crate::app) fn system_theme(&mut self, mode: SystemMode) {
        match mode {
            SystemMode::Dark => self.system_dark = true,
            SystemMode::Light => self.system_dark = false,
            SystemMode::None => return,
        }
        if self.follow_system {
            self.dark = self.system_dark;
        }
    }
}

/// The settings that choose a font or its size (RC-010-17), how the lists shorten a name, and
/// whether DOS text's lines are as tall as the viewer font's block.
const FONT_KEYS: [&str; 10] = [
    DOS_TIGHT_LINES_KEY,
    LIST_FONT_KEY,
    LIST_FONT_SIZE_KEY,
    MARK_HIDDEN_KEY,
    NAME_CUT_KEY,
    NAME_TIP_DELAY_KEY,
    TERMINAL_FONT_KEY,
    TERMINAL_FONT_SIZE_KEY,
    VIEWER_FONT_KEY,
    VIEWER_FONT_SIZE_KEY,
];
