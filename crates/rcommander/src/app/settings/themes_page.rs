//! The Themes tab of the Appearance page (RC-100-50; the Colours tab of RC-100-43 before): the
//! theme of the light and of the dark mode, each a pick list that edits the draft like any
//! setting; the list of every theme with a tile of its colours and New, Duplicate, Rename, Edit
//! and Delete; a file field with Import and Export of a `.rctheme` file; and the colours by file
//! type, which stay global.
//!
//! Only own themes change: New and Duplicate make one (`theme.save`) and open it in the editor
//! (`editor.rs`), whose live preview recolours the window; the theme a mode draws with is not
//! deleted. A file is read and written off the window's thread (`file.rs`) and checked whole by
//! the engine's rules before it is saved; a name another own theme has asks whether to replace
//! it, to take another name or to stop. What the tab reads - the themes, the chosen ones, the
//! rules - is applied to the window at the start and on every change ([`read`]); the colours of
//! RC-100-43 become a Custom theme on the way ([`adopt_colours`]). The themes of the connections'
//! panels and whether they apply (RC-100-51, `theme/themes/connections.rs`) are read with them,
//! again after every change of the connection tree.

mod actions;
mod editor;
mod editor_view;
mod file;
mod labels;
mod view;

use iced::Task;
use rc_db::Db;
use rc_engine::settings::{
    COLOUR_KEYS, COLOUR_RULES_KEY, CONNECTION_THEMES_KEY, THEME_DARK_KEY, THEME_LIGHT_KEY,
    stored_bool, stored_setting,
    themes::{self, ImportedTheme, Theme, ThemesOutcome, ThemesState, adopt_colours},
};
use serde_json::Value;

pub use self::editor::EditorMsg;
pub(super) use self::view::view;
use super::{SettingsMessage, wrap};
use crate::{
    app::{App, Effect, Message},
    theme,
};

/// What the tab and the window read: the themes, the chosen ones and the rules by file type.
#[derive(Debug, Clone)]
pub struct ThemesRead {
    pub state: ThemesState,
    pub rules: String,
    /// The connections' themes and `appearance.connection_themes` (RC-100-51).
    pub connections: theme::themes::Assigned,
}

/// Takes the colours of RC-100-43 into themes, then reads the themes and the rules.
pub async fn read(db: Db) -> ThemesRead {
    if let Err(error) = adopt_colours(&db).await {
        tracing::warn!(%error, "the chosen list colours cannot become a theme");
    }
    let state = themes::load(&db).await;
    let rules = stored_setting(&db, COLOUR_RULES_KEY)
        .await
        .as_ref()
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let rows = db.connection_themes().await.unwrap_or_else(|error| {
        tracing::warn!(%error, "the connections' themes cannot be read; the window's apply");
        Vec::new()
    });
    let on = stored_bool(&db, CONNECTION_THEMES_KEY).await;
    let connections = theme::themes::Assigned::of(on, rows);
    ThemesRead {
        state,
        rules,
        connections,
    }
}

/// Whether the setting `key` changes what the themes draw: a mode's theme, a colour of RC-100-43
/// (an import writes them), the rules by file type (one of [`COLOUR_KEYS`]), the connections'
/// themes switched on or off (RC-100-51).
pub fn concerns(key: &str) -> bool {
    key == THEME_LIGHT_KEY
        || key == THEME_DARK_KEY
        || key == CONNECTION_THEMES_KEY
        || COLOUR_KEYS.contains(&key)
}

/// An import whose name an own theme has: the theme read and the one there.
#[derive(Debug, Clone)]
pub struct Conflict {
    imported: ImportedTheme,
    existing: Theme,
}

/// What the person answers to an import's name taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictChoice {
    Replace,
    Rename,
    Cancel,
}

/// The tab's state while the dialog is open.
#[derive(Debug, Default)]
pub struct ThemesPage {
    /// The themes as last read; `None` until then.
    state: Option<ThemesState>,
    /// The theme marked in the list.
    selected: Option<String>,
    /// The new name typed while the marked theme is renamed.
    renaming: Option<String>,
    /// The file Import reads and Export writes.
    file: String,
    conflict: Option<Box<Conflict>>,
    /// The last outcome: a text, and whether it is an error.
    note: Option<(String, bool)>,
    /// A save, a delete or a file runs; the buttons wait.
    busy: bool,
    editor: Option<editor::Editor>,
}

impl ThemesPage {
    /// The theme marked in the list, as last read.
    fn marked(&self) -> Option<&Theme> {
        let state = self.state.as_ref()?;
        state.theme(self.selected.as_deref()?)
    }

    /// Whether a mode draws with the theme `id`.
    fn chosen(&self, id: &str) -> bool {
        self.state
            .as_ref()
            .is_some_and(|state| state.chosen.iter().any(|chosen| chosen == id))
    }
}

/// What came of a `theme` command the tab sent.
#[derive(Debug, Clone)]
pub struct Answer {
    pub result: Result<Box<ThemesOutcome>, &'static str>,
    /// The note on success.
    pub note: String,
    /// The theme saved, to open in the editor under the id it got.
    pub edit: Option<Box<Theme>>,
    /// The editor saved: it closes on success.
    pub close_editor: bool,
}

#[derive(Debug, Clone)]
pub enum ThemesMsg {
    /// The themes and the rules, read at the start and after a change.
    Read(Box<ThemesRead>),
    /// A theme marked in the list.
    Choose(String),
    New,
    Duplicate,
    Edit,
    Delete,
    Rename,
    RenameText(String),
    RenameDone,
    RenameCancel,
    /// The file field typed or browsed.
    File(String),
    Import,
    Export,
    /// The file read and checked; the error's code otherwise.
    Imported(Result<Box<ImportedTheme>, &'static str>),
    /// The file written; the error's code otherwise.
    Exported(Result<String, &'static str>),
    Conflict(ConflictChoice),
    Answered(Box<Answer>),
    Editor(EditorMsg),
}

/// The message of the tab.
pub(super) fn page_message(message: ThemesMsg) -> Message {
    wrap(SettingsMessage::Themes(message))
}

impl App {
    /// Reads the themes and the rules (RC-100-50): at the start, after a change of one of them
    /// and when the dialog opens.
    pub(in crate::app) fn themes_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(read(db), |read| {
            page_message(ThemesMsg::Read(Box::new(read)))
        })
    }

    /// The themes read: the window draws with them, the terminals take their colours, the tab
    /// lists them.
    fn themes_read(&mut self, read: ThemesRead) -> Vec<Effect> {
        theme::themes::set_state(&read.state);
        theme::themes::set_assigned(read.connections);
        theme::colour_rules::set_rules(&read.rules);
        self.restyle_terminals();
        if let Some(page) = self.settings.as_mut().map(|dialog| &mut dialog.themes) {
            let dark = self.dark;
            let keep = page
                .selected
                .as_ref()
                .is_some_and(|id| read.state.theme(id).is_some());
            if !keep {
                page.selected = Some(read.state.chosen[usize::from(dark)].clone());
            }
            page.state = Some(read.state);
        }
        Vec::new()
    }

    /// A message of the Themes tab.
    pub(super) fn themes_message(&mut self, message: ThemesMsg) -> Vec<Effect> {
        match message {
            ThemesMsg::Read(read) => self.themes_read(*read),
            ThemesMsg::Editor(message) => self.theme_editor_message(message),
            ThemesMsg::Answered(answer) => self.themes_answered(*answer),
            other => self.themes_action(other),
        }
    }
}

#[cfg(test)]
#[path = "themes_page_tests.rs"]
mod tests;
