//! The find-files dialog (Total Commander's Alt+F7, RC-020-03): masks, start folders and a text
//! on the first tab, size, date and attributes on the second, the hits below as they are found.
//!
//! Alt+F7 is `files.find_dialog`; the engine announces it and the dialog opens over the window
//! for the panel. Start runs [`Engine::start_search`] in the background (`stream.rs`): its
//! events arrive in batches and land in the hit list ([`Results`], at most what a panel can be
//! fed); Stop or Escape cancel it. "Go to file" (a double-click) shows the hit's folder in the
//! panel with the cursor on it; "Feed to listbox" shows every hit flat in the panel
//! (`panel.feed_paths`), where copy, move and delete act on the real files. F3 and F4 go to the
//! hit and hand the key to the panel. Closing keeps the form and the hits: Alt+F7 shows them
//! again, the start folder set to the panel's. The masks searched are kept, newest first, in
//! one setting (`search.history`), as the mask prompt keeps its own.
//!
//! While the dialog is open it has the keyboard and the mouse; the panels get nothing.

mod form;
mod results;
mod stream;
mod view;

use iced::Task;
use rc_engine::{
    Chord, Command, CursorTarget, EngineEvent, PanelId, SearchCancel, SearchEvent, SearchStats,
    VfsPath, search::KindFilter,
};

pub(super) use self::view::{body, flat_badge, overlay};
pub use self::{
    form::{Attribute, Field, Flag, Form, Tri},
    results::Results,
};
use super::{App, Effect, Message, mru};
use crate::i18n::error_text;

/// The dialog's two tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    General,
    Advanced,
}

/// Where the search stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Run {
    Idle,
    Running {
        dir: Option<VfsPath>,
        stats: SearchStats,
    },
    Finished {
        stats: SearchStats,
        cancelled: bool,
    },
    /// The search did not start: a start folder is missing, a pattern does not compile.
    Failed(String),
}

/// The dialog, open or closed with its last search kept.
#[derive(Debug)]
pub struct SearchDialog {
    pub open: bool,
    /// The panel it searches from and goes to.
    pub panel: PanelId,
    pub tab: Tab,
    pub form: Form,
    pub results: Results,
    pub run: Run,
    /// The field that did not read when Start was pressed.
    pub invalid: Option<Field>,
    /// Masks searched, newest first.
    pub history: Vec<String>,
    /// The first start folder of the last search, absolute: the folder fed hits are shown in.
    root: Option<String>,
    /// Numbers the searches, so a late batch of an earlier one is dropped.
    generation: u64,
    cancel: Option<SearchCancel>,
    /// Stop was asked before the search had started.
    stop_wanted: bool,
}

impl SearchDialog {
    fn new(panel: PanelId, start: String) -> Self {
        Self {
            open: true,
            panel,
            tab: Tab::General,
            form: Form::starting_in(start),
            results: Results::default(),
            run: Run::Idle,
            invalid: None,
            history: Vec::new(),
            root: None,
            generation: 0,
            cancel: None,
            stop_wanted: false,
        }
    }

    pub fn running(&self) -> bool {
        matches!(self.run, Run::Running { .. })
    }

    fn stop(&mut self) {
        self.stop_wanted = true;
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
    }

    /// One batch of what the running search reported.
    fn apply(&mut self, updates: Vec<Update>) {
        for update in updates {
            match update {
                Update::Started(cancel) => {
                    if self.stop_wanted {
                        cancel.cancel();
                    }
                    self.cancel = Some(cancel);
                }
                Update::Failed(code) => {
                    self.run = Run::Failed(error_text(code));
                    self.cancel = None;
                }
                Update::Event(SearchEvent::Hit(hit)) => {
                    if !self.results.push(hit) {
                        self.stop();
                    }
                }
                Update::Event(SearchEvent::Progress { dir, stats }) => {
                    if self.running() {
                        self.run = Run::Running { dir, stats };
                    }
                }
                Update::Event(SearchEvent::Skipped { .. }) => {}
                Update::Event(SearchEvent::Finished { stats, cancelled }) => {
                    self.run = Run::Finished { stats, cancelled };
                    self.cancel = None;
                }
            }
        }
    }
}

/// What a running search sends the dialog.
#[derive(Debug, Clone)]
pub enum Update {
    Started(SearchCancel),
    /// The error code of a search that did not start.
    Failed(&'static str),
    Event(SearchEvent),
}

/// What the dialog's controls and its search say.
#[derive(Debug, Clone)]
pub enum SearchMsg {
    Tab(Tab),
    Text(Field, String),
    Flag(Flag, bool),
    Attribute(Attribute, Tri),
    Kind(KindFilter),
    /// A kept mask chosen.
    Recall(String),
    /// The kept masks, read at the first open.
    History(Vec<String>),
    Start,
    Stop,
    Close,
    Feed,
    GoTo,
    /// A hit clicked, by its index.
    Click(usize),
    /// A hit double-clicked: go to it.
    Open(usize),
    /// The wheel over the hits, in rows.
    Scroll(isize),
    /// The hit list is this many rows high.
    Viewport(usize),
    /// A batch of the search numbered so.
    Updates(u64, Vec<Update>),
}

impl App {
    /// While the dialog is open, keys and clicks belong to it; Alt+F7 opens it. `None` for a
    /// message the dialog does not take.
    pub(super) fn search_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        if let Message::Engine(EngineEvent::FindFiles { panel }) = message {
            return Some(self.open_search(*panel));
        }
        self.search.as_ref().filter(|dialog| dialog.open)?;
        match message {
            Message::Key(chord) => Some(self.find_key(chord, false)),
            Message::KeyCaptured(chord) => Some(self.find_key(chord, true)),
            Message::Press => Some(Vec::new()),
            _ => None,
        }
    }

    /// Opens the dialog for `panel`: a new one on the panel's folder, or the last one with its
    /// hits, now starting in the panel's folder.
    fn open_search(&mut self, panel: PanelId) -> Vec<Effect> {
        let start = self.pane(panel).state.path.path.display().to_string();
        if let Some(dialog) = self.search.as_mut() {
            dialog.open = true;
            if !dialog.running() {
                dialog.panel = panel;
                dialog.form.start = start;
            }
            // In a window of its own, Alt+F7 brings it to the front (RC-080-15).
            return self.dialog_window_front(super::dialog_windows::DialogWindow::Search);
        }
        self.search = Some(SearchDialog::new(panel, start));
        let Some(db) = self.db.clone() else {
            return Vec::new();
        };
        let load = Task::perform(async move { mru::SEARCH.load(&db).await }, |history| {
            Message::Search(SearchMsg::History(history))
        });
        vec![Effect::Task(load)]
    }

    pub(super) fn on_search(&mut self, message: SearchMsg) -> Vec<Effect> {
        let Some(dialog) = self.search.as_mut() else {
            return Vec::new();
        };
        match message {
            SearchMsg::Tab(tab) => dialog.tab = tab,
            SearchMsg::Text(field, text) => {
                *dialog.form.text_mut(field) = text;
                if dialog.invalid == Some(field) {
                    dialog.invalid = None;
                }
            }
            SearchMsg::Flag(flag, on) => *dialog.form.flag_mut(flag) = on,
            SearchMsg::Attribute(attribute, tri) => *dialog.form.tri_mut(attribute) = tri,
            SearchMsg::Kind(kind) => dialog.form.kind = kind,
            SearchMsg::Recall(mask) => dialog.form.name = mask,
            SearchMsg::History(history) => dialog.history = mru::SEARCH.sanitized(history),
            SearchMsg::Start => return self.start_find(),
            SearchMsg::Stop => dialog.stop(),
            SearchMsg::Close => self.close_search(),
            SearchMsg::Feed => return self.feed_hits(),
            SearchMsg::GoTo => return self.go_to_hit(),
            SearchMsg::Click(index) => dialog.results.point_at(index),
            SearchMsg::Open(index) => {
                dialog.results.point_at(index);
                return self.go_to_hit();
            }
            SearchMsg::Scroll(rows) => dialog.results.scroll(rows),
            SearchMsg::Viewport(rows) => dialog.results.set_page(rows),
            SearchMsg::Updates(generation, updates) => {
                if dialog.generation == generation {
                    dialog.apply(updates);
                }
            }
        }
        Vec::new()
    }

    /// A key while the dialog is open: Enter starts, Escape stops a search or closes, the
    /// cursor keys move in the hits, F3 and F4 go to the hit and hand the key to the panel.
    /// A key a text field took (`captured`) only counts for Escape.
    fn find_key(&mut self, chord: &Chord, captured: bool) -> Vec<Effect> {
        if !captured && let Some(effects) = super::field_walk::walk_key(chord) {
            return effects;
        }
        if !chord.is_plain() {
            return Vec::new();
        }
        let Some(dialog) = self.search.as_mut() else {
            return Vec::new();
        };
        let page = dialog.results.page() as isize - 1;
        match chord.key.as_str() {
            "Escape" if dialog.running() => dialog.stop(),
            "Escape" => self.close_search(),
            _ if captured => {}
            "Enter" => return self.start_find(),
            "ArrowUp" => dialog.results.step(-1),
            "ArrowDown" => dialog.results.step(1),
            "PageUp" => dialog.results.step(-page),
            "PageDown" => dialog.results.step(page),
            "F3" | "F4" => {
                let mut effects = self.go_to_hit();
                if !effects.is_empty() {
                    effects.extend(self.on_key(chord.clone(), false));
                }
                return effects;
            }
            _ => {}
        }
        Vec::new()
    }

    fn start_find(&mut self) -> Vec<Effect> {
        let Some(dialog) = self.search.as_mut().filter(|dialog| !dialog.running()) else {
            return Vec::new();
        };
        let query = match dialog.form.query() {
            Ok(query) => query,
            Err(field) => {
                dialog.invalid = Some(field);
                return Vec::new();
            }
        };
        let paths = dialog.form.start_folders();
        let folder = &self.panes[dialog.panel.index()].state.path.path;
        dialog.root = paths
            .first()
            .map(|first| folder.join(first).display().to_string());
        dialog.invalid = None;
        dialog.generation += 1;
        dialog.results.clear();
        dialog.run = Run::Running {
            dir: None,
            stats: SearchStats::default(),
        };
        dialog.cancel = None;
        dialog.stop_wanted = false;
        mru::SEARCH.remember(&mut dialog.history, dialog.form.name.trim());
        let search = stream::search(
            self.engine.clone(),
            dialog.panel,
            paths,
            query,
            dialog.generation,
        );
        let mut effects = vec![Effect::Task(search)];
        if let Some(db) = self.db.clone() {
            effects.push(Effect::Task(
                mru::SEARCH.save_task(db, dialog.history.clone()),
            ));
        }
        effects
    }

    /// Hides the dialog, its form and hits kept; a running search stops.
    fn close_search(&mut self) {
        if let Some(dialog) = self.search.as_mut() {
            dialog.stop();
            dialog.open = false;
        }
    }

    /// "Go to file": the panel shows the hit's folder, the cursor on the hit; the dialog closes.
    fn go_to_hit(&mut self) -> Vec<Effect> {
        let Some(dialog) = self.search.as_ref() else {
            return Vec::new();
        };
        let panel = dialog.panel;
        let Some(hit) = dialog.results.current() else {
            return Vec::new();
        };
        let (Some(folder), Some(name)) = (hit.path.parent(), hit.path.path.file_name()) else {
            return Vec::new();
        };
        let mut commands = self.activating(panel);
        commands.push(Command::Navigate {
            panel: Some(panel),
            path: folder.path.display().to_string(),
        });
        commands.push(Command::SetCursor {
            panel: Some(panel),
            target: CursorTarget::Name(name.to_string_lossy().into_owned()),
        });
        self.close_search();
        vec![Effect::Dispatch(panel, commands)]
    }

    /// "Feed to listbox": every hit flat in the panel, in the first start folder.
    fn feed_hits(&mut self) -> Vec<Effect> {
        let Some(dialog) = self
            .search
            .as_ref()
            .filter(|dialog| !dialog.results.is_empty())
        else {
            return Vec::new();
        };
        let panel = dialog.panel;
        let paths = dialog
            .results
            .hits()
            .iter()
            .map(|hit| hit.path.path.display().to_string())
            .collect();
        let mut commands = self.activating(panel);
        commands.push(Command::FeedPaths {
            panel: Some(panel),
            root: dialog.root.clone(),
            paths,
        });
        self.close_search();
        vec![Effect::Dispatch(panel, commands)]
    }

    /// The command that makes `panel` the active one, unless it is.
    fn activating(&self, panel: PanelId) -> Vec<Command> {
        if panel == self.active {
            Vec::new()
        } else {
            vec![Command::SwitchActivePanel { panel: Some(panel) }]
        }
    }
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
