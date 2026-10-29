//! The command line under the panels, the quick search and the quick filter (RC-010-08), as in
//! Total Commander.
//!
//! Letters typed over a panel go into the command line; Enter runs it in the active panel's
//! folder through `cmdline.run` (`cd` and a drive move the panel, anything else opens a console
//! window; Shift+Enter keeps it open), Escape clears it. Ctrl+Enter writes the name under the
//! cursor into the line, Ctrl+Shift+Enter its full path and Ctrl+P the panel's folder, quoted
//! where they need it. The lines run are kept (`mru::CMDLINE`): ArrowUp and ArrowDown walk them
//! while the field has the keyboard (after a click into it, or typing), Alt+F8 lists them. In an
//! archive or on a server the line is disabled and says so.
//!
//! Ctrl+Alt with letters starts the quick search (the setting `quick_search.keys` makes it Alt
//! with letters, or letters alone - which then no longer go into the line): the cursor jumps to
//! the first name that matches, ArrowDown and ArrowUp to the next and previous match, Escape or
//! any other key ends it; with `quick_search.filters` it hides what does not match instead, as the
//! quick filter does. Ctrl+S types a quick filter into the same place (`panel.filter`): only
//! the matching entries show, the filter stays after Enter and shows in the path bar, Escape lifts
//! it; Ctrl+Shift+S applies the last one again. The matching is the engine's
//! (`rc_engine::quick_search`).
//!
//! A key press that types a character reaches the window as [`CmdLineMsg::Typed`]: it is routed
//! as the key it is - so every dialog and menu that takes keys still does - and only a key that
//! reaches the panels gives its character to the line or the search (`keys.rs`).

mod keys;
mod view;

use iced::{
    Subscription, Task,
    advanced::widget::{operate, operation::focusable},
    event::{self, Event},
    mouse,
    widget::{Id, operation},
};
use rc_db::Db;
use rc_engine::{
    Chord, Command, ConnectionId, PanelId,
    command_line::{self, Parsed, quote, runs_in},
    quick_search::MatchOptions,
    settings::{
        QUICK_SEARCH_ANYWHERE_KEY, QUICK_SEARCH_CASE_KEY, QUICK_SEARCH_FILTERS_KEY,
        QUICK_SEARCH_KEYS_KEY, stored_bool, stored_setting,
    },
};
use rc_vfs::BackendId;

pub(super) use keys::key_message;
pub(super) use view::with_filter;

use super::{App, Effect, Message, mru};

/// The id of the command line's text field, to focus it.
pub const CMDLINE_ID: &str = "rcommander.cmdline";

/// The id of the Alt+F8 list's scrollable, which keeps its highlighted line in view.
const HISTORY_SCROLL_ID: &str = "rcommander.cmdline.history";

/// Which keys start the quick search (`quick_search.keys`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SearchKeys {
    /// Ctrl+Alt with a letter, Total Commander's default; letters alone go into the line.
    #[default]
    CtrlAlt,
    /// Alt with a letter.
    Alt,
    /// Letters alone, while the line is empty and has not got the keyboard.
    Letters,
}

/// The quick search's settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    pub keys: SearchKeys,
    pub matching: MatchOptions,
    /// `quick_search.filters`: the quick search hides what does not match.
    pub filters: bool,
}

/// What is typed in place of the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    /// The quick search: the cursor follows the text.
    Find,
    /// The quick filter: the list follows the text.
    Filter,
}

/// A quick search or filter being typed for `panel`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Search {
    pub panel: PanelId,
    pub kind: SearchKind,
    pub text: String,
    /// Started by the quick search's keys: its text matches as the quick search's does, also
    /// when it filters.
    pub quick: bool,
}

/// The command line and the quick search.
#[derive(Debug, Default)]
pub struct CmdLine {
    /// The line typed so far.
    pub text: String,
    /// The lines run, newest first.
    pub history: Vec<String>,
    /// The kept line shown, by its place in the history; `None` while typed.
    recalled: Option<usize>,
    /// Whether the field has the keyboard: a click into it, or typing into it, gave it.
    focused: bool,
    /// Whether the pointer is over the field.
    hovered: bool,
    /// The Alt+F8 list, open, with its highlighted line.
    pub list: Option<usize>,
    /// The quick search or filter being typed, if any.
    pub search: Option<Search>,
    pub options: Options,
    /// The character of the key press now on its way ([`CmdLineMsg::Typed`]).
    pending: Option<String>,
}

#[derive(Debug, Clone)]
pub enum CmdLineMsg {
    /// A key press that typed `text`: routed as the key, the text for the line or the search.
    Typed(Chord, String),
    /// The field's text changed while it had the keyboard.
    Input(String),
    /// The pointer entered (`true`) or left the field.
    Hover(bool),
    /// A mouse button went down somewhere: the field has the keyboard when it was the field.
    Press,
    /// A line of the Alt+F8 list clicked.
    Pick(usize),
    /// The stored settings, and at the start the stored lines.
    Loaded(Options, Option<Vec<String>>),
}

fn wrap(message: CmdLineMsg) -> Message {
    Message::CmdLine(message)
}

/// Whether the setting `key` is one of the quick search's.
pub(super) fn concerns(key: &str) -> bool {
    key.starts_with("quick_search.")
}

/// The stored settings and lines, read off the UI thread at the start.
pub(super) fn load(db: Option<Db>) -> Task<Message> {
    Task::perform(
        async move {
            match db {
                Some(db) => (options(&db).await, Some(mru::CMDLINE.load(&db).await)),
                None => (Options::default(), None),
            }
        },
        |(options, history)| wrap(CmdLineMsg::Loaded(options, history)),
    )
}

/// The quick search's settings as stored; the defaults where nothing (valid) is.
async fn options(db: &Db) -> Options {
    let keys = stored_setting(db, QUICK_SEARCH_KEYS_KEY).await;
    let keys = match keys.as_ref().and_then(|value| value.as_str()) {
        Some("alt") => SearchKeys::Alt,
        Some("letters") => SearchKeys::Letters,
        _ => SearchKeys::CtrlAlt,
    };
    Options {
        keys,
        matching: MatchOptions {
            anywhere: stored_bool(db, QUICK_SEARCH_ANYWHERE_KEY).await,
            case_sensitive: stored_bool(db, QUICK_SEARCH_CASE_KEY).await,
        },
        filters: stored_bool(db, QUICK_SEARCH_FILTERS_KEY).await,
    }
}

/// Every mouse press, so the line knows whether a click gave its field the keyboard.
pub(super) fn presses() -> Subscription<Message> {
    event::listen_with(|event, _status, _window| match event {
        Event::Mouse(mouse::Event::ButtonPressed(_)) => Some(wrap(CmdLineMsg::Press)),
        _ => None,
    })
}

/// Focuses the field with the cursor at the end of its text.
fn focus_field() -> Effect {
    let id = Id::new(CMDLINE_ID);
    Effect::Task(Task::batch([
        operation::focus(id.clone()),
        operation::move_cursor_to_end(id),
    ]))
}

/// Takes the keyboard from the field (and any other text field).
fn unfocus_field() -> Effect {
    Effect::Task(operate(focusable::unfocus::<()>()).discard())
}

impl App {
    pub(super) fn on_cmdline(&mut self, message: CmdLineMsg) -> Vec<Effect> {
        match message {
            CmdLineMsg::Typed(chord, text) => {
                self.cmdline.pending = Some(text);
                let effects = self.apply(Message::Key(chord));
                self.cmdline.pending = None;
                return effects;
            }
            CmdLineMsg::Input(text) => {
                self.cmdline.recalled = None;
                self.cmdline.text = text;
                // An emptied line gives the keys back to the panel: Backspace goes up again.
                if self.cmdline.text.is_empty() {
                    return self.leave_field();
                }
            }
            CmdLineMsg::Hover(over) => self.cmdline.hovered = over,
            CmdLineMsg::Press => {
                self.cmdline.focused = self.cmdline.hovered && self.line_enabled();
            }
            CmdLineMsg::Pick(index) => return self.pick_history(index),
            CmdLineMsg::Loaded(options, history) => {
                self.cmdline.options = options;
                if let Some(history) = history {
                    self.cmdline.history = history;
                }
            }
        }
        Vec::new()
    }

    /// A quick-search setting changed: all of them are read again.
    pub(super) fn cmdline_settings(&self) -> Vec<Effect> {
        let Some(db) = self.db.clone() else {
            return Vec::new();
        };
        let task = Task::perform(async move { options(&db).await }, |options| {
            wrap(CmdLineMsg::Loaded(options, None))
        });
        vec![Effect::Task(task)]
    }

    /// Whether the active panel's folder takes a command line.
    fn line_enabled(&self) -> bool {
        runs_in(&self.pane(self.active).state.path)
    }

    /// Enter (Shift+Enter keeps the console open): the line runs for the active panel and is
    /// kept; the field is emptied and the keys go back to the panel.
    fn run_line(&mut self, keep_open: bool) -> Vec<Effect> {
        let line = std::mem::take(&mut self.cmdline.text);
        mru::CMDLINE.remember(&mut self.cmdline.history, line.trim());
        let panel = self.active;
        // On an SFTP panel a program runs on its server: the "Run command" dialog takes the
        // line, and Enter there runs it (RC-060-04); `cd` and a drive stay the engine's.
        if let BackendId::Sftp { connection_id } = self.pane(panel).state.path.backend
            && let Ok(Parsed::Run(command)) = command_line::check(&line).map(command_line::parse)
        {
            let command = command.to_owned();
            let mut effects = self.leave_field();
            effects.extend(self.save_history());
            effects.extend(self.open_ssh_exec(ConnectionId(connection_id), None, command));
            return effects;
        }
        let run = Command::RunCommandLine {
            panel: Some(panel),
            line,
            keep_open,
        };
        let mut effects = vec![Effect::Dispatch(panel, vec![run])];
        effects.extend(self.leave_field());
        effects.extend(self.save_history());
        effects
    }

    /// Escape: the line is emptied and the keys go back to the panel.
    fn clear_line(&mut self) -> Vec<Effect> {
        self.cmdline.text.clear();
        self.leave_field()
    }

    /// The panel has the keys again.
    fn leave_field(&mut self) -> Vec<Effect> {
        self.cmdline.recalled = None;
        if std::mem::take(&mut self.cmdline.focused) {
            vec![unfocus_field()]
        } else {
            Vec::new()
        }
    }

    /// Ctrl+Enter (`full`: Ctrl+Shift+Enter): the name (path) under the cursor, quoted where it
    /// needs it, added to the line; nothing on the `..` row.
    fn insert_name(&mut self, full: bool) -> Vec<Effect> {
        let pane = self.pane(self.active);
        let Some(entry) = pane.state.cursor_entry().filter(|_| !pane.on_parent()) else {
            return Vec::new();
        };
        let word = if full {
            let path = pane.state.path.join(&entry.name).path;
            quote(&path.to_string_lossy()).into_owned()
        } else {
            quote(&entry.name).into_owned()
        };
        self.insert(&word)
    }

    /// Ctrl+P: the panel's folder added to the line.
    fn insert_folder(&mut self) -> Vec<Effect> {
        let folder = self.pane(self.active).state.path.path.to_string_lossy();
        let word = quote(&folder).into_owned();
        self.insert(&word)
    }

    /// `word` and a blank after what is typed, as Total Commander adds it.
    fn insert(&mut self, word: &str) -> Vec<Effect> {
        let text = &mut self.cmdline.text;
        if !text.is_empty() && !text.ends_with(' ') {
            text.push(' ');
        }
        text.push_str(word);
        text.push(' ');
        self.cmdline.recalled = None;
        if self.cmdline.focused {
            vec![focus_field()]
        } else {
            Vec::new()
        }
    }

    /// Typed characters added to the line, which takes the keyboard.
    fn type_into_line(&mut self, typed: &str) -> Vec<Effect> {
        self.cmdline.text.push_str(typed);
        self.cmdline.recalled = None;
        self.cmdline.focused = true;
        vec![focus_field()]
    }

    /// ArrowUp (`by` 1, older) or ArrowDown (-1, newer) in the field: the next kept line; below
    /// the newest the line is empty again.
    fn recall_line(&mut self, by: isize) -> Vec<Effect> {
        let next = match self.cmdline.recalled {
            None if by > 0 => Some(0),
            None => return Vec::new(),
            Some(at) => at.checked_add_signed(by),
        };
        match next.and_then(|at| self.cmdline.history.get(at).map(|line| (at, line))) {
            Some((at, line)) => {
                self.cmdline.text.clone_from(line);
                self.cmdline.recalled = Some(at);
            }
            None if by < 0 => {
                self.cmdline.text.clear();
                self.cmdline.recalled = None;
            }
            None => return Vec::new(),
        }
        vec![focus_field()]
    }

    /// Alt+F8: the kept lines, the newest highlighted.
    fn open_list(&mut self) -> Vec<Effect> {
        self.cmdline.list = Some(0);
        self.history_in_view()
    }

    /// The open Alt+F8 list scrolled so its highlighted line is in view.
    fn history_in_view(&self) -> Vec<Effect> {
        match self.cmdline.list {
            Some(at) => vec![super::scroll::keep_in_view(
                HISTORY_SCROLL_ID,
                at,
                self.cmdline.history.len(),
            )],
            None => Vec::new(),
        }
    }

    /// A kept line chosen: it becomes the line, which takes the keyboard.
    fn pick_history(&mut self, index: usize) -> Vec<Effect> {
        self.cmdline.list = None;
        let Some(line) = self.cmdline.history.get(index).cloned() else {
            return Vec::new();
        };
        self.cmdline.text = line;
        self.cmdline.recalled = Some(index);
        self.cmdline.focused = true;
        vec![focus_field()]
    }

    /// Stores the kept lines; one that cannot be stored is only lost at the next start.
    fn save_history(&self) -> Option<Effect> {
        let db = self.db.clone()?;
        Some(Effect::Task(
            mru::CMDLINE.save_task(db, self.cmdline.history.clone()),
        ))
    }
}

#[cfg(test)]
#[path = "cmdline_tests.rs"]
mod tests;
