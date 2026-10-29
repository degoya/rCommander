//! The settings dialog, after Total Commander's Configuration: a list of pages on the left, the
//! page on the right, OK, Cancel and Apply below - and the light or dark look it sets.
//!
//! The pages draw the engine's schema ([`rc_engine::SETTINGS`]), so every setting appears;
//! retention and the MCP rights and token have pages of their own. Apply stores only what
//! changed: an ordinary setting through `settings.set`, a protected one (the MCP switch and
//! port, the vault's idle lock) through [`rc_engine::Engine::set_protected_setting`], the policy
//! and the token in the database - none of them through anything MCP can call. A change of the
//! switch, the port or the token restarts the MCP server at once. The theme follows the engine's
//! [`EngineEvent::SettingChanged`], so a change by an agent shows too; so do the fonts of the
//! lists, the terminal and the viewer (RC-010-17), which the Appearance page chooses from the
//! installed ones (`fonts_page.rs`). The Import page opens the
//! import from Total Commander (RC-050-10), which the connection manager draws over the dialog.
//! The About page (RC-010-19) shows the build and the licences; it needs no database. The Host
//! keys page (RC-050-07, `host_keys.rs`) lists, searches, removes and imports the trusted SSH
//! host keys and FTPS certificates through the engine. The Plugins page (RC-100-09,
//! `plugins.rs`) lists the installed plugins, installs one after its preview, switches, rolls
//! back, resumes and removes them, and sets their own settings; its switches and limits are
//! schema rows like any other. The Plugins and Appearance pages are split into tabs (RC-100-20,
//! `tabs.rs`); the tab each shows is the app's for the session. The Keyboard page's Shortcuts tab
//! changes the keys (RC-100-21, `shortcuts.rs`). What the window applies at once when a setting
//! changes - the keys among it - is `apply.rs`.

mod about;
mod apply;
mod colours_page;
mod draft;
mod fonts_page;
mod host_keys;
mod host_keys_view;
mod mcp_page;
mod plugins;
mod plugins_view;
mod save;
mod saving;
mod shortcuts;
mod shortcuts_view;
mod tabs;
mod view;

use iced::Task;
use rc_engine::CommandClass;
use rc_mcp::{McpToken, Rule};

pub(super) use self::about::{window_icon, window_title};
pub use self::draft::{Changes, Limit, Loaded};
use self::draft::{Draft, Field};
pub use self::host_keys::TrustStoreCall;
pub use self::plugins::{PluginsCall, PluginsMsg};
pub use self::save::Stored;
pub use self::shortcuts::ShortcutsMsg;
pub use self::tabs::{Tab, Tabs};
// The read the dialog's task runs, for the tests that draw the pages (`field_walk`).
#[cfg(test)]
pub(super) use self::save::load;
use super::{
    App, Effect, Message,
    escape::{enter, escape},
    field_walk,
};
use crate::{
    i18n::{error_text, t},
    mcp::McpStatus,
    theme::{self, ThemeChoice, fonts::Fonts},
};

/// The dialog's pages, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Appearance,
    Folders,
    /// What F10 does (RC-080-17).
    Keyboard,
    Terminal,
    Vault,
    /// The trusted SSH host keys and FTPS certificates (RC-050-07).
    HostKeys,
    Retention,
    Mcp,
    /// The external viewer and editor (RC-010-07).
    Programs,
    /// Copies and moves: buffer and how many run at once (RC-060-05).
    Transfers,
    /// The installed plugins and their switches (RC-100-09).
    Plugins,
    /// Imports from other programs: Total Commander's (RC-050-10).
    Import,
    /// The self-update: its state, the offer and its notes, the switches (RC-100-05).
    Updates,
    /// The build and the licences (RC-010-19).
    About,
}

impl Page {
    pub const ALL: [Self; 14] = [
        Self::Appearance,
        Self::Folders,
        Self::Keyboard,
        Self::Terminal,
        Self::Vault,
        Self::HostKeys,
        Self::Retention,
        Self::Mcp,
        Self::Programs,
        Self::Transfers,
        Self::Plugins,
        Self::Import,
        Self::Updates,
        Self::About,
    ];
}

#[derive(Debug, Clone)]
pub enum SettingsMessage {
    Open,
    /// The dialog opens on this page (the palette's "About rCommander").
    OpenPage(Page),
    About(about::AboutMsg),
    HostKeys(host_keys::HostKeysMsg),
    Plugins(PluginsMsg),
    /// The Shortcuts tab (RC-100-21).
    Shortcuts(ShortcutsMsg),
    /// The keymap in force, read at a change of `keys.overrides`.
    Keymap(Box<rc_engine::Keymap>),
    Loaded(Result<Box<Loaded>, &'static str>),
    Page(Page),
    /// A tab of the page chosen (RC-100-20).
    Tab(Tab),
    Toggle(&'static str, bool),
    Edit(&'static str, String),
    Retention(usize, Limit, String),
    Rule(CommandClass, Rule),
    CopyToken,
    TokenLoaded(Result<McpToken, &'static str>),
    RotateToken,
    TokenRotated(Result<McpStatus, &'static str>),
    Apply,
    Ok,
    Cancel,
    /// Apply or OK finished: the MCP server's new state when it was restarted, and a failure.
    Saved {
        result: Stored,
        changes: Box<Changes>,
        close: bool,
    },
    /// The stored theme, read after it changed.
    Theme(ThemeChoice),
    /// The stored `appearance.alternate_rows`, read at the start and after it changed.
    Stripes(bool),
    /// The font lists offer monospaced families only, or all.
    MonoOnly(bool),
    /// The fonts the settings choose, resolved; read at the start and after one changed.
    Fonts(Box<Fonts>),
    /// The colours and the rules by file type, read at the start and after one changed
    /// (RC-100-43).
    Colours(Box<colours_page::ColourSettings>),
    /// Opens the system's colour dialog for a colour of the Colours tab.
    Pick(colours_page::PickTarget),
    /// The colour the dialog gave, `#rrggbb`; `None` when it was cancelled.
    Picked(colours_page::PickTarget, Option<String>),
    /// An edit of the colour rule at this place of the rule list (RC-080-53).
    ColourRule(usize, colours_page::RuleOp),
}

/// The open dialog.
#[derive(Debug)]
pub struct SettingsDialog {
    page: Page,
    /// What is edited; `None` until loaded.
    draft: Option<Draft>,
    /// The last outcome: a text, and whether it is an error.
    note: Option<(String, bool)>,
    /// A load or a save runs; the buttons wait.
    busy: bool,
    /// A save of this dialog runs: its answer is this dialog's (`saving.rs`).
    saving: bool,
    about: about::AboutPage,
    host_keys: host_keys::HostKeysPage,
    plugins: plugins::PluginsPage,
    shortcuts: shortcuts::ShortcutsPage,
    /// What the Updates page shows, read when the dialog opens and after each change.
    updates: Option<Box<rc_engine::UpdateStatus>>,
}

impl SettingsDialog {
    fn new() -> Self {
        Self {
            page: Page::Appearance,
            draft: None,
            note: None,
            busy: true,
            saving: false,
            about: about::AboutPage::default(),
            host_keys: host_keys::HostKeysPage::default(),
            plugins: plugins::PluginsPage::default(),
            shortcuts: shortcuts::ShortcutsPage::default(),
            updates: None,
        }
    }

    /// What the Updates page shows, once read.
    pub(in crate::app) fn updates(&self) -> Option<&rc_engine::UpdateStatus> {
        self.updates.as_deref()
    }

    /// The self-update's state for the Updates page and the About page's facts.
    pub(in crate::app) fn set_updates(&mut self, status: Box<rc_engine::UpdateStatus>) {
        self.about.set_update_facts(&status);
        self.updates = Some(status);
    }

    fn fail(&mut self, code: &str) {
        self.busy = false;
        self.note = Some((error_text(code), true));
    }

    #[cfg(test)]
    pub(super) fn page(&self) -> Page {
        self.page
    }
}

fn wrap(message: SettingsMessage) -> Message {
    Message::Settings(message)
}

impl App {
    /// A message of the settings dialog.
    pub(super) fn settings_message(&mut self, message: SettingsMessage) -> Vec<Effect> {
        if let SettingsMessage::Open = message {
            return self.open_settings();
        }
        if let SettingsMessage::OpenPage(page) = message {
            let mut effects = self.open_settings();
            if let Some(dialog) = self.settings.as_mut() {
                dialog.page = page;
            }
            // A menu entry opens its page on the first tab ("Plugins...": Installed).
            self.settings_tabs.first(page);
            effects.extend(self.host_keys_shown());
            effects.extend(self.plugins_shown());
            return effects;
        }
        if let SettingsMessage::About(message) = message {
            return self.about_message(message);
        }
        if let SettingsMessage::HostKeys(message) = message {
            return self.host_keys_message(message);
        }
        if let SettingsMessage::Plugins(message) = message {
            return self.plugins_message(message);
        }
        if let SettingsMessage::Shortcuts(message) = message {
            return self.shortcuts_message(message);
        }
        if let SettingsMessage::Keymap(keymap) = message {
            return self.keymap_read(*keymap);
        }
        if let SettingsMessage::Theme(choice) = message {
            return self.set_theme(choice);
        }
        if let SettingsMessage::Stripes(on) = message {
            theme::set_stripes(on);
            return Vec::new();
        }
        // The Colours tab: what is stored applied, the colour dialog opened and answered.
        let message = match self.colours_message(message) {
            Ok(effects) => return effects,
            Err(message) => message,
        };
        if let SettingsMessage::Fonts(fonts) = message {
            theme::fonts::set_fonts(*fonts);
            self.refont_terminals();
            return Vec::new();
        }
        if let SettingsMessage::Cancel = message {
            self.settings = None;
            return Vec::new();
        }
        // Also when the dialog closed while it saved: the result is the window's then.
        if let SettingsMessage::Saved {
            result,
            changes,
            close,
        } = message
        {
            return self.settings_saved(result, &changes, close);
        }
        if let SettingsMessage::Tab(tab) = message {
            self.settings_tabs.choose(tab);
            return Vec::new();
        }
        // Turning a page needs no database (the About page shows none of it, the Host keys page
        // reads the engine's).
        if let SettingsMessage::Page(page) = message {
            if let Some(dialog) = self.settings.as_mut() {
                dialog.page = page;
            }
            let mut effects = self.host_keys_shown();
            effects.extend(self.plugins_shown());
            return effects;
        }
        let (Some(dialog), Some(db)) = (self.settings.as_mut(), self.db.clone()) else {
            return Vec::new();
        };
        let engine = self.engine.clone();
        match message {
            SettingsMessage::Loaded(Ok(loaded)) => {
                dialog.draft = Some(Draft::new(*loaded));
                dialog.busy = false;
            }
            SettingsMessage::Loaded(Err(code)) => dialog.fail(code),
            SettingsMessage::Toggle(key, on) => {
                if let Some(draft) = &mut dialog.draft {
                    draft.edit(key, Field::Bool(on));
                }
            }
            SettingsMessage::MonoOnly(on) => {
                if let Some(draft) = &mut dialog.draft {
                    draft.mono_only = on;
                }
            }
            SettingsMessage::Edit(key, text) => {
                if let Some(draft) = &mut dialog.draft {
                    draft.edit(key, Field::Text(text));
                }
            }
            SettingsMessage::Retention(index, limit, text) => {
                if let Some(draft) = &mut dialog.draft {
                    draft.edit_retention(index, limit, text);
                }
            }
            SettingsMessage::Rule(class, rule) => {
                if let Some(draft) = &mut dialog.draft {
                    draft.set_rule(class, rule);
                }
            }
            SettingsMessage::CopyToken => {
                let task = Task::perform(save::token(db), |token| {
                    wrap(SettingsMessage::TokenLoaded(token))
                });
                return vec![Effect::Task(task)];
            }
            SettingsMessage::TokenLoaded(Ok(token)) => {
                dialog.note = Some((t!("settings.mcp.token_copied").to_owned(), false));
                return vec![Effect::Task(iced::clipboard::write(
                    token.expose().to_owned(),
                ))];
            }
            SettingsMessage::TokenLoaded(Err(code)) | SettingsMessage::TokenRotated(Err(code)) => {
                dialog.fail(code);
            }
            SettingsMessage::RotateToken => {
                dialog.busy = true;
                let server = self.mcp_server.clone();
                let task = Task::perform(save::rotate(engine, db, server), |status| {
                    wrap(SettingsMessage::TokenRotated(status))
                });
                return vec![Effect::Task(task)];
            }
            SettingsMessage::TokenRotated(Ok(status)) => {
                dialog.busy = false;
                dialog.note = Some((t!("settings.mcp.token_rotated").to_owned(), false));
                self.mcp = status;
            }
            SettingsMessage::Apply => return self.save_settings(false),
            SettingsMessage::Ok => return self.save_settings(true),
            SettingsMessage::Open
            | SettingsMessage::OpenPage(_)
            | SettingsMessage::About(_)
            | SettingsMessage::HostKeys(_)
            | SettingsMessage::Plugins(_)
            | SettingsMessage::Shortcuts(_)
            | SettingsMessage::Keymap(_)
            | SettingsMessage::Theme(_)
            | SettingsMessage::Stripes(_)
            | SettingsMessage::Fonts(_)
            | SettingsMessage::Colours(_)
            | SettingsMessage::Pick(_)
            | SettingsMessage::Picked(..)
            | SettingsMessage::ColourRule(..)
            | SettingsMessage::Page(_)
            | SettingsMessage::Tab(_)
            | SettingsMessage::Saved { .. }
            | SettingsMessage::Cancel => {}
        }
        Vec::new()
    }

    fn open_settings(&mut self) -> Vec<Effect> {
        let mut dialog = SettingsDialog::new();
        let Some(db) = self.db.clone() else {
            dialog.fail("database_unavailable");
            self.settings = Some(dialog);
            return Vec::new();
        };
        self.settings = Some(dialog);
        let task = Task::perform(save::load(self.engine.clone(), db), |loaded| {
            wrap(SettingsMessage::Loaded(loaded.map(Box::new)))
        });
        vec![
            Effect::Task(task),
            super::update_ui::status_task(&self.engine),
        ]
    }

    /// While the dialog is open, keys and clicks belong to it: Escape closes it, Enter is OK (as
    /// in Total Commander), Tab and Shift+Tab walk the page's fields, Ctrl+Tab and Ctrl+Shift+Tab
    /// turn the page's tabs as in a Windows dialog; nothing reaches the panels.
    /// `None` for a message the dialog does not take.
    pub(super) fn settings_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        let busy = self.settings.as_ref()?.busy;
        // The import the Import page opened is over the dialog and has the keys.
        if self
            .connections
            .as_ref()
            .is_some_and(|manager| manager.over_settings())
        {
            return None;
        }
        // The Host keys page's own keys: its rows and the question before a removal.
        if let Message::Key(chord) = message
            && let Some(effects) = self
                .host_keys_key(chord)
                .or_else(|| self.shortcuts_key(chord))
        {
            return Some(effects);
        }
        match message {
            Message::Key(chord) if *chord == escape() => {
                self.settings = None;
                Some(Vec::new())
            }
            Message::Key(chord) if *chord == enter() => Some(if busy {
                Vec::new()
            } else {
                self.save_settings(true)
            }),
            Message::Key(chord) if field_walk::tab(chord).is_some() => field_walk::walk_key(chord),
            Message::Key(chord) if chord.key == "Tab" && chord.ctrl && !chord.alt => {
                let dialog = self.settings.as_mut()?;
                if dialog.page == Page::About {
                    dialog.about.turn(chord.shift);
                } else {
                    let page = dialog.page;
                    self.settings_tabs.turn(page, chord.shift);
                }
                Some(Vec::new())
            }
            Message::Key(_) | Message::Press => Some(Vec::new()),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
