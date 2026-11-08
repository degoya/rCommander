//! "Choose symbol..." (RC-110-10): the symbols of rCommander's own collection
//! (`rc_engine::user_commands::symbols`, Fluent UI System Icons) to pick one for an own command or
//! a bar button, as Total Commander's "Change icon" dialog offers `WCMICONS.DLL`. A page of the
//! window that opened it - the own commands' or the button bars' -, in place of its form while it
//! is open: the search over names and English keywords, the shelves on the left (recently used,
//! all, the nine categories), the grid of symbols at the bars' size, the preview with the
//! button's tooltip, OK and Cancel in one row. OK hands the window `rc-symbol:<name>`; the file
//! and its index stay the window's own fields, the other way to a symbol.
//!
//! "Recently used" is what the stored own commands and bars name already - no table of its own.
//! The search narrows the symbols shown while a word grows and looks at the shelf again only
//! when it changes otherwise ([`SymbolPicker::examined`] counts what it looked at).
//!
//! The keyboard: it starts in the search; Tab and Shift+Tab walk the ring (search, shelves, grid,
//! OK, Cancel); the arrows move in the grid (Up and Down from the search too), choose the shelf
//! in the shelves; Home, End, PageUp and PageDown move in the grid; Enter takes the symbol at the
//! cursor; Escape goes back to the form.

pub mod view;

use iced::widget::Id;
use rc_engine::{
    Chord,
    user_commands::symbols::{self, Category, Symbol},
};

use super::{App, Effect, Message, field_walk, scroll};

/// The search: the keyboard starts here.
pub const SEARCH_FIELD: &str = "rcommander.symbol_picker.search";
/// The shelves as one stop of the focus ring.
pub const SHELVES_STOP_ID: &str = "rcommander.symbol_picker.shelves";
/// The grid as one stop of the focus ring.
pub const GRID_STOP_ID: &str = "rcommander.symbol_picker.grid";
/// The grid's scrollable, kept on the cursor.
pub const GRID_SCROLL_ID: &str = "rcommander.symbol_picker.grid.scroll";
/// The grid's columns: Up and Down move by as many.
pub const COLUMNS: usize = 10;
/// The rows PageUp and PageDown move by.
const PAGE_ROWS: usize = 5;
/// The most symbols on the shelf of the recently used.
pub const MAX_RECENT: usize = 20;

/// A shelf on the left: what the grid shows before the search narrows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shelf {
    /// The symbols the stored own commands and bars name.
    Recent,
    All,
    Category(Category),
}

impl Shelf {
    /// Every shelf, in the list's order.
    pub fn list() -> Vec<Self> {
        let mut shelves = vec![Self::Recent, Self::All];
        shelves.extend(Category::ALL.into_iter().map(Self::Category));
        shelves
    }
}

/// Where the picker's keys go: the stop of its focus ring that has the keyboard.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PickFocus {
    #[default]
    Search,
    Shelves,
    Grid,
    /// OK or Cancel: their keys are their own.
    Ring,
}

/// What the picker's controls say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerMsg {
    Query(String),
    Shelf(Shelf),
    /// A symbol of the grid clicked: the cursor on it.
    Select(usize),
    /// A symbol of the grid double-clicked: taken.
    Choose(usize),
    /// The cursor moved by places in the grid.
    Move(isize),
    Ok,
    Cancel,
    /// After Tab: the id of the stop that has the keyboard.
    Focused(Option<Id>),
}

/// What a message leaves the window that opened the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Still open; `true` when the cursor moved, so the grid scrolls to it.
    Open(bool),
    /// `rc-symbol:<name>` taken.
    Chosen(String),
    Cancelled,
}

/// The open picker.
#[derive(Debug, Clone)]
pub struct SymbolPicker {
    pub query: String,
    pub shelf: Shelf,
    /// The symbols the grid shows, as places in `symbols::all()`.
    pub shown: Vec<usize>,
    /// The cursor, as a place in `shown`.
    pub cursor: Option<usize>,
    /// The symbols the stored commands and bars name, each once, as places in `symbols::all()`.
    pub recent: Vec<usize>,
    /// The tooltip of the command or button the symbol is for: the preview shows it.
    pub tooltip: String,
    /// The bars' symbol size: the grid's and the preview's.
    pub size: u16,
    pub focus: PickFocus,
    /// The words the last filtering looked for, in lower case.
    words: Vec<String>,
    /// How many symbols the last filtering looked at.
    pub examined: usize,
}

impl SymbolPicker {
    /// The picker for `current` (an `icon_path`: its symbol at the cursor when it names one) of a
    /// command or button with `tooltip`, at the bars' `size`; `used` the `icon_path`s the stored
    /// own commands and bars carry, the most recent first.
    pub fn open<'u>(
        current: &str,
        tooltip: &str,
        size: u16,
        used: impl IntoIterator<Item = &'u str>,
    ) -> Self {
        let mut recent: Vec<usize> = Vec::new();
        for path in used {
            if let Some(at) = symbols::name_of(path).and_then(symbols::position)
                && !recent.contains(&at)
            {
                recent.push(at);
            }
            if recent.len() == MAX_RECENT {
                break;
            }
        }
        let mut picker = Self {
            query: String::new(),
            shelf: Shelf::All,
            shown: Vec::new(),
            cursor: None,
            recent,
            tooltip: tooltip.trim().to_owned(),
            size,
            focus: PickFocus::Search,
            words: Vec::new(),
            examined: 0,
        };
        picker.refilter();
        let at = symbols::name_of(current).and_then(symbols::position);
        picker.cursor = match at {
            Some(at) => picker.shown.iter().position(|shown| *shown == at),
            None => (!picker.shown.is_empty()).then_some(0),
        };
        picker
    }

    /// The symbol at the cursor.
    pub fn chosen(&self) -> Option<&'static Symbol> {
        let at = *self.shown.get(self.cursor?)?;
        symbols::all().get(at)
    }

    /// The places of `symbols::all()` the shelf holds.
    fn shelf_places(&self) -> Vec<usize> {
        match self.shelf {
            Shelf::Recent => self.recent.clone(),
            Shelf::All => (0..symbols::all().len()).collect(),
            Shelf::Category(category) => symbols::all()
                .iter()
                .enumerate()
                .filter(|(_, symbol)| symbol.category == category)
                .map(|(at, _)| at)
                .collect(),
        }
    }

    /// The symbols shown for the query: the shelf's, narrowed by the words. A query that only
    /// grows a word or adds one looks at the symbols shown already, any other at the shelf.
    fn filter(&mut self, words: Vec<String>, narrowing: bool) {
        let candidates = if narrowing {
            std::mem::take(&mut self.shown)
        } else {
            self.shelf_places()
        };
        self.examined = candidates.len();
        let all = symbols::all();
        self.shown = candidates
            .into_iter()
            .filter(|at| all.get(*at).is_some_and(|symbol| symbol.matches(&words)))
            .collect();
        self.words = words;
    }

    /// The shelf read again with the query.
    fn refilter(&mut self) {
        self.filter(words_of(&self.query), false);
    }

    /// Whether `words` only narrow the last words: each old word grown, new ones after them.
    fn narrows(&self, words: &[String]) -> bool {
        words.len() >= self.words.len()
            && self
                .words
                .iter()
                .zip(words)
                .all(|(old, new)| new.contains(old.as_str()))
    }

    /// The cursor kept on the symbol it was on, else on the first.
    fn keep_cursor(&mut self, was: Option<usize>) {
        self.cursor = was
            .and_then(|at| self.shown.iter().position(|shown| *shown == at))
            .or((!self.shown.is_empty()).then_some(0));
    }

    /// The row of the cursor and the grid's rows, for scrolling to it.
    pub fn cursor_row(&self) -> (usize, usize) {
        let rows = self.shown.len().div_ceil(COLUMNS);
        (self.cursor.unwrap_or(0) / COLUMNS, rows)
    }

    pub fn update(&mut self, message: PickerMsg) -> Outcome {
        let was = self.cursor.and_then(|at| self.shown.get(at).copied());
        match message {
            PickerMsg::Query(query) => {
                self.query = query;
                let words = words_of(&self.query);
                let narrowing = self.narrows(&words);
                self.filter(words, narrowing);
                self.keep_cursor(was);
            }
            PickerMsg::Shelf(shelf) => {
                self.shelf = shelf;
                self.refilter();
                self.keep_cursor(was);
            }
            PickerMsg::Select(at) => {
                self.cursor = (at < self.shown.len()).then_some(at).or(self.cursor);
                self.focus = PickFocus::Grid;
                // A click is on a symbol in view: the grid stays where it is.
                return Outcome::Open(false);
            }
            PickerMsg::Choose(at) => {
                if at < self.shown.len() {
                    self.cursor = Some(at);
                    return self.take();
                }
            }
            PickerMsg::Move(by) => {
                if self.shown.is_empty() {
                    return Outcome::Open(false);
                }
                let last = self.shown.len() - 1;
                let to = match by {
                    isize::MIN => 0,
                    isize::MAX => last,
                    by => self.cursor.unwrap_or(0).saturating_add_signed(by).min(last),
                };
                let moved = self.cursor != Some(to);
                self.cursor = Some(to);
                return Outcome::Open(moved);
            }
            PickerMsg::Ok => return self.take(),
            PickerMsg::Cancel => return Outcome::Cancelled,
            PickerMsg::Focused(id) => self.focus = focus_of(id.as_ref()),
        }
        Outcome::Open(true)
    }

    /// OK: the symbol at the cursor; nothing while the grid is empty.
    fn take(&self) -> Outcome {
        match self.chosen() {
            Some(symbol) => Outcome::Chosen(symbol.reference()),
            None => Outcome::Open(false),
        }
    }
}

/// The words of a query, in lower case.
fn words_of(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

/// The keys follow the ring to the stop `id`.
pub fn focus_of(id: Option<&Id>) -> PickFocus {
    let is = |name: &'static str| id == Some(&Id::new(name));
    if is(SEARCH_FIELD) {
        PickFocus::Search
    } else if is(SHELVES_STOP_ID) {
        PickFocus::Shelves
    } else if is(GRID_STOP_ID) {
        PickFocus::Grid
    } else {
        PickFocus::Ring
    }
}

/// What a key asks of the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyAction {
    Nothing,
    /// Tab (`false`) or Shift+Tab (`true`): the next stop of the ring.
    Walk(bool),
    /// An access key moved the ring: which stop has the keyboard now.
    Follow,
    Run(PickerMsg),
}

/// A key at the stop `focus`; `captured` when a text field or a stop of the ring used it already.
pub fn key(focus: PickFocus, chord: &Chord, captured: bool) -> KeyAction {
    if captured {
        return if chord.alt && !chord.ctrl {
            KeyAction::Follow
        } else {
            KeyAction::Nothing
        };
    }
    if let Some(back) = field_walk::tab(chord) {
        return KeyAction::Walk(back);
    }
    if !chord.is_plain() {
        return KeyAction::Nothing;
    }
    let columns = COLUMNS as isize;
    let page = columns * PAGE_ROWS as isize;
    let grid = focus == PickFocus::Grid;
    let message = match (chord.key.as_str(), focus) {
        ("Escape", _) => PickerMsg::Cancel,
        (_, PickFocus::Ring) => return KeyAction::Nothing,
        ("Enter", _) => PickerMsg::Ok,
        ("ArrowLeft", _) if grid => PickerMsg::Move(-1),
        ("ArrowRight", _) if grid => PickerMsg::Move(1),
        ("Home", _) if grid => PickerMsg::Move(isize::MIN),
        ("End", _) if grid => PickerMsg::Move(isize::MAX),
        ("ArrowUp", PickFocus::Grid | PickFocus::Search) => PickerMsg::Move(-columns),
        ("ArrowDown", PickFocus::Grid | PickFocus::Search) => PickerMsg::Move(columns),
        ("PageUp", PickFocus::Grid | PickFocus::Search) => PickerMsg::Move(-page),
        ("PageDown", PickFocus::Grid | PickFocus::Search) => PickerMsg::Move(page),
        _ => return KeyAction::Nothing,
    };
    KeyAction::Run(message)
}

/// What a key's `action` does in the window that opened the picker: a walk of the ring, after
/// which `moved` says where the keys went, or `run` with a message of the picker.
pub fn effects(
    action: KeyAction,
    moved: fn(Option<Id>) -> Message,
    run: impl FnOnce(PickerMsg) -> Vec<Effect>,
) -> Vec<Effect> {
    match action {
        KeyAction::Nothing => Vec::new(),
        KeyAction::Walk(back) => vec![Effect::WalkFields {
            back,
            then: Some(moved),
        }],
        KeyAction::Follow => vec![Effect::Task(field_walk::focused().map(moved))],
        KeyAction::Run(message) => run(message),
    }
}

/// The grid scrolled so the cursor's row is in view.
pub fn follow(picker: &SymbolPicker) -> Effect {
    let (row, rows) = picker.cursor_row();
    scroll::keep_in_view(GRID_SCROLL_ID, row, rows)
}

/// The keyboard to the search, as the picker opens.
pub fn focus_search() -> Effect {
    Effect::Task(iced::widget::operation::focus(Id::new(SEARCH_FIELD)))
}

impl App {
    /// The picker for `current` (an `icon_path`) of a command or button with `tooltip`: at the
    /// bars' size, recently used what the stored own commands - the newest first - and the bars'
    /// buttons name.
    pub(in crate::app) fn symbol_picker(&self, current: &str, tooltip: &str) -> SymbolPicker {
        let mut commands = super::user_commands::known::commands();
        commands.sort_by_key(|command| std::cmp::Reverse(command.id));
        let buttons = self
            .button_bar
            .bars
            .iter()
            .flat_map(|bar| bar.spec.items.iter())
            .filter_map(|item| item.button().and_then(|button| button.icon.as_ref()))
            .map(|icon| icon.path.as_str());
        let used = commands
            .iter()
            .map(|command| command.icon_path.as_str())
            .chain(buttons);
        SymbolPicker::open(current, tooltip, self.button_bar.settings.icon_size, used)
    }
}

#[cfg(test)]
#[path = "symbol_picker_tests.rs"]
mod tests;
