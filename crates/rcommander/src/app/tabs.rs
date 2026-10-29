//! Folder tabs, the folder history and the hotlist in the window (RC-010-03).
//!
//! Tabs, history and hotlist are the engine's; the window keeps a copy of each panel's tab row,
//! read again whenever the engine announces a change of the panel, and draws it above the panel
//! when there are two tabs or more (`view.rs`). A click shows a tab, a middle click closes it, a
//! right click opens its menu (lock, copy to the other panel, close; each with its key,
//! RC-010-24). A row too wide for its panel shows the tabs that fit, the active one always among
//! them, and `»` listing every tab (`overflow.rs`, RC-010-21). A tab dragged within its bar takes
//! another place (`reorder.rs`, RC-010-24).
//!
//! Two keys open a popup instead of running their command at once, as in Total Commander:
//! Alt+Down (`history.list`) lists the tab's folders, Ctrl+D (`hotlist.list`) the hotlist with
//! "add current folder" and "configure" (`popup.rs`). The hotlist dialog behind "configure" is
//! `editor.rs`. A popup or the dialog has the keyboard while it is open.
//!
//! Ctrl+Shift+A, a window key of the keymap, lists the active panel's tabs as `»` does, also
//! with a single tab; while the panel shows its terminal it lists the terminal's sessions, the
//! bar above it, and the terminal lets go of the keyboard until the list closes (RC-080-16).

pub(super) mod editor;
mod overflow;
pub(super) mod popup;
mod reorder;
mod view;

use iced::{
    Task,
    advanced::widget::{operate, operation::focusable},
};
use rc_engine::{
    AddHotlistRequest, Chord, Command, Engine, HistoryJumpRequest, HotlistEntry, LockTabRequest,
    OpenHotlistRequest, OpenTabRequest, Outcome, PanelId, PanelView, Relocated, SelectTabRequest,
    TabId, TabRequest, TabView, TransferDirRequest,
};

pub use self::editor::FolderChoice;
pub use self::reorder::DragMsg;
pub(super) use self::reorder::pointer_events;
pub(super) use self::view::{editor_body as hotlist_body, tab_bar, with_tabs_overlay};
use self::{
    editor::HotlistEditor,
    popup::{Popup, PopupAction, PopupKey},
};
use super::{App, Effect, Message, scroll};
use crate::i18n::t;

/// The id of a popup's scrollable, which keeps the popup's cursor in view.
const POPUP_SCROLL_ID: &str = "tabs.popup";

/// The window key that lists the active panel's tabs, or its terminal sessions.
pub const LIST_KEY: &str = "tab_list";

/// What the window keeps of the tabs and the hotlist.
#[derive(Debug, Default)]
pub struct TabsUi {
    /// Each panel's tab row as the engine last reported it, by [`PanelId::index`].
    pub strips: [Vec<TabView>; 2],
    /// The open popup.
    pub popup: Option<Popup>,
    /// The hotlist dialog, while it is open.
    pub editor: Option<HotlistEditor>,
    /// Ctrl+D was pressed in this panel; the menu opens when the hotlist is read.
    menu_for: Option<PanelId>,
    /// A tab pressed, and dragged once the pointer went far enough (RC-010-24).
    drag: Option<reorder::TabDrag>,
    /// `tabs.drag_reorder` is off: a press on a tab only shows it.
    no_drag: bool,
}

impl TabsUi {
    /// The tab rows of `engine`'s panels.
    pub fn new(engine: &Engine) -> Self {
        Self {
            strips: PanelId::BOTH.map(|panel| engine.tabs(panel)),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone)]
pub enum TabsMessage {
    /// A click on a tab.
    Select(PanelId, TabId),
    /// A middle click on a tab.
    Close(PanelId, TabId),
    /// A right click on a tab: its menu.
    Menu(PanelId, TabId),
    /// A click on `»` of a row too wide for its panel: the list of all its tabs.
    List(PanelId),
    /// A tab pressed, dragged, dropped (RC-010-24).
    Drag(DragMsg),
    /// `tabs.drag_reorder` and `keys.context_menu_shortcuts`, read.
    Settings {
        drag: bool,
        keys: bool,
    },
    /// A click on a popup's row.
    Pick(usize),
    /// A click beside the popup.
    ClosePopup,
    /// The hotlist was read, or the error code of the read.
    HotlistRead(Result<Vec<HotlistEntry>, &'static str>),
    /// A row of the hotlist dialog was clicked.
    Choose(usize),
    Name(String),
    Path(String),
    Target(String),
    Save,
    AddCurrent,
    AddFolder,
    Up,
    Down,
    MoveTo(FolderChoice),
    Delete,
    CloseEditor,
}

/// The start-up note on stored tabs whose folder was gone; a place inside an archive is named
/// through the archive file (`C:\data\backup.zip\docs`).
pub fn relocated_note(relocated: &[Relocated]) -> Option<String> {
    match relocated {
        [] => None,
        [one] => Some(t!(
            "tabs.relocated",
            stored = rc_engine::shown_path(&one.stored).display(),
            shown = rc_engine::shown_path(&one.shown).display()
        )),
        many => Some(t!("tabs.relocated_many", count = many.len())),
    }
}

impl App {
    /// The engine announced a change of `panel`: its tab row is read again.
    pub(super) fn refresh_tabs(&mut self, panel: PanelId) {
        self.tabs.strips[panel.index()] = self.engine.tabs(panel);
    }

    /// Whether a popup, the hotlist dialog or a tab being dragged has the keyboard.
    pub(super) fn tabs_have_keys(&self) -> bool {
        self.tabs.popup.is_some() || self.tabs.editor.is_some() || self.tab_dragged()
    }

    /// Whether a tab is being dragged (past the threshold, RC-010-24).
    fn tab_dragged(&self) -> bool {
        self.tabs.drag.is_some_and(|drag| drag.dragging)
    }

    /// A key while a popup or the dialog is open.
    pub(super) fn tabs_key(&mut self, chord: &Chord) -> Vec<Effect> {
        if self.tab_dragged() {
            return self.tab_drag_key(&chord.key);
        }
        if self.tabs.editor.is_some() {
            // Tab and Shift+Tab walk the hotlist dialog's focus ring (RC-080-16).
            if let Some(effects) = super::field_walk::walk_key(chord) {
                return effects;
            }
            if chord.key == "Escape" {
                self.tabs.editor = None;
            }
            return Vec::new();
        }
        let Some(popup) = self.tabs.popup.as_mut() else {
            return Vec::new();
        };
        let panel = popup.panel;
        match popup.key(chord) {
            PopupKey::Stay => self.popup_in_view(),
            PopupKey::Close => {
                self.tabs.popup = None;
                vec![Effect::FocusTerminal(panel)]
            }
            PopupKey::Take(action) => {
                self.tabs.popup = None;
                let mut effects = self.popup_action(panel, action);
                effects.push(Effect::FocusTerminal(panel));
                effects
            }
            PopupKey::TabMenu(tab) => self.on_tabs(TabsMessage::Menu(panel, tab)),
            PopupKey::MoveTab(tab, to) => {
                let mut effects = self.move_tab(panel, tab, to);
                effects.extend(self.popup_in_view());
                effects
            }
        }
    }

    /// Ctrl+Shift+A: the list of `panel`'s tabs, or of its terminal sessions while it shows its
    /// terminal, which lets go of the keyboard meanwhile.
    pub(super) fn open_tab_list(&mut self, panel: PanelId) -> Vec<Effect> {
        if self.pane(panel).state.view != PanelView::Terminal {
            return self.on_tabs(TabsMessage::List(panel));
        }
        let sessions = self
            .engine
            .panel_terminals(panel)
            .into_iter()
            .filter(|session| !session.state.is_exited())
            .map(|session| (session.terminal, session.title, session.shown));
        self.tabs.popup = Some(Popup::sessions(panel, sessions));
        let mut effects = self.popup_in_view();
        effects.push(Effect::Task(operate(focusable::unfocus::<()>()).discard()));
        effects
    }

    /// A key's command that the tabs handle in the window first: Alt+Down and Ctrl+D open their
    /// popup; Ctrl+Up and Ctrl+Left/Right on the `..` row take the folder above. `None` lets the
    /// command run as it is.
    pub(super) fn tabs_command(&mut self, command: &Command) -> Option<Vec<Effect>> {
        let active = self.active;
        match command {
            Command::HistoryList(_) => {
                let (entries, current) = self.engine.history(active);
                self.tabs.popup = Some(Popup::history(active, &entries, current));
                Some(self.popup_in_view())
            }
            Command::ListHotlist {} => {
                self.tabs.menu_for = Some(active);
                Some(self.read_hotlist())
            }
            Command::OpenTab(request) if request.cursor && self.pane(active).on_parent() => {
                let command = Command::OpenTab(OpenTabRequest {
                    path: Some("..".into()),
                    cursor: false,
                    ..request.clone()
                });
                Some(vec![Effect::Dispatch(active, vec![command])])
            }
            Command::TransferDir(request) if request.path.is_none() => {
                let source = request.panel.unwrap_or(active);
                if !self.pane(source).on_parent() {
                    return None;
                }
                let command = Command::TransferDir(TransferDirRequest {
                    path: Some("..".into()),
                    ..request.clone()
                });
                Some(vec![Effect::Dispatch(active, vec![command])])
            }
            _ => None,
        }
    }

    /// The engine announced a change of the hotlist: an open dialog reads it again.
    pub(super) fn hotlist_changed(&mut self) -> Vec<Effect> {
        if self.tabs.editor.is_some() {
            self.read_hotlist()
        } else {
            Vec::new()
        }
    }

    pub(super) fn on_tabs(&mut self, message: TabsMessage) -> Vec<Effect> {
        match message {
            TabsMessage::Select(panel, tab) => {
                let mut commands = self.activate(panel);
                commands.push(Command::SelectTab(SelectTabRequest {
                    panel: Some(panel),
                    tab,
                }));
                vec![Effect::Dispatch(panel, commands)]
            }
            TabsMessage::Close(panel, tab) => self.popup_action(panel, PopupAction::CloseTab(tab)),
            TabsMessage::Menu(panel, tab) => {
                let locked = self.tabs.strips[panel.index()]
                    .iter()
                    .any(|view| view.id == tab && view.locked);
                let keymap = &self.keymap;
                let key_of = |id: &str| super::context_menu::keys::of_command(keymap, id);
                self.tabs.popup = Some(Popup::tab_menu(panel, tab, locked, key_of));
                Vec::new()
            }
            TabsMessage::List(panel) => {
                self.tabs.popup = Some(Popup::tabs(panel, &self.tabs.strips[panel.index()]));
                self.popup_in_view()
            }
            TabsMessage::Pick(index) => {
                let Some(popup) = self.tabs.popup.take() else {
                    return Vec::new();
                };
                match popup.take(index) {
                    PopupKey::Take(action) => {
                        let mut effects = self.popup_action(popup.panel, action);
                        effects.push(Effect::FocusTerminal(popup.panel));
                        effects
                    }
                    _ => {
                        self.tabs.popup = Some(popup);
                        Vec::new()
                    }
                }
            }
            TabsMessage::ClosePopup => match self.tabs.popup.take() {
                Some(popup) => vec![Effect::FocusTerminal(popup.panel)],
                None => Vec::new(),
            },
            TabsMessage::HotlistRead(read) => self.hotlist_read(read),
            TabsMessage::Drag(message) => self.on_tab_drag(message),
            TabsMessage::Settings { drag, keys } => {
                self.tabs_settings(drag, keys);
                Vec::new()
            }
            other => self.editor_message(other),
        }
    }

    /// A message of the hotlist dialog.
    fn editor_message(&mut self, message: TabsMessage) -> Vec<Effect> {
        let active = self.active;
        let Some(editor) = self.tabs.editor.as_mut() else {
            return Vec::new();
        };
        let command = match message {
            TabsMessage::Choose(index) => {
                editor.choose(index);
                None
            }
            TabsMessage::Name(text) => {
                editor.name = text;
                None
            }
            TabsMessage::Path(text) => {
                editor.path = text;
                None
            }
            TabsMessage::Target(text) => {
                editor.target = text;
                None
            }
            TabsMessage::Save => editor.save(),
            TabsMessage::AddCurrent => Some(editor.add_current(active)),
            TabsMessage::AddFolder => Some(editor.add_folder()),
            TabsMessage::Up => editor.step(-1),
            TabsMessage::Down => editor.step(1),
            TabsMessage::MoveTo(choice) => editor.move_to(choice.id),
            TabsMessage::Delete => editor.delete(),
            TabsMessage::CloseEditor => {
                self.tabs.editor = None;
                None
            }
            _ => None,
        };
        command
            .map(|command| Effect::Dispatch(active, vec![command]))
            .into_iter()
            .collect()
    }

    /// What a popup row does for `panel`.
    fn popup_action(&mut self, panel: PanelId, action: PopupAction) -> Vec<Effect> {
        let command = match action {
            PopupAction::HistoryJump(index) => Command::HistoryJump(HistoryJumpRequest {
                panel: Some(panel),
                index,
            }),
            PopupAction::OpenHotlist(id) => Command::OpenHotlist(OpenHotlistRequest {
                id,
                panel: Some(panel),
            }),
            PopupAction::AddCurrent => Command::AddHotlist(AddHotlistRequest {
                panel: Some(panel),
                ..AddHotlistRequest::default()
            }),
            PopupAction::Configure => {
                self.tabs.editor = Some(HotlistEditor::default());
                return self.read_hotlist();
            }
            PopupAction::SelectTab(tab) => return self.on_tabs(TabsMessage::Select(panel, tab)),
            PopupAction::LockTab(tab) => Command::LockTab(LockTabRequest {
                panel: Some(panel),
                tab: Some(tab),
                locked: None,
            }),
            PopupAction::CopyTab(tab) => Command::CopyTabToOther(TabRequest {
                panel: Some(panel),
                tab: Some(tab),
            }),
            PopupAction::CloseTab(tab) => Command::CloseTab(TabRequest {
                panel: Some(panel),
                tab: Some(tab),
            }),
            PopupAction::ShowTerminal(terminal) => Command::ShowTerminal { terminal },
            PopupAction::CloseTerminal(terminal) => Command::CloseTerminal { terminal },
        };
        vec![Effect::Dispatch(panel, vec![command])]
    }

    /// `panel` becomes active first when it is not.
    fn activate(&self, panel: PanelId) -> Vec<Command> {
        if panel == self.active {
            Vec::new()
        } else {
            vec![Command::SwitchActivePanel { panel: Some(panel) }]
        }
    }

    /// Reads the hotlist off the UI thread.
    fn read_hotlist(&self) -> Vec<Effect> {
        let engine = self.engine.clone();
        let read = Task::perform(
            async move {
                match engine.dispatch(Command::ListHotlist {}).await {
                    Ok(Outcome::Hotlist { items }) => Ok(items),
                    Ok(_) => Ok(Vec::new()),
                    Err(error) => Err(error.code()),
                }
            },
            |read| Message::Tabs(TabsMessage::HotlistRead(read)),
        );
        vec![Effect::Task(read)]
    }

    /// The hotlist arrived: the waiting menu opens, an open dialog shows it.
    fn hotlist_read(&mut self, read: Result<Vec<HotlistEntry>, &'static str>) -> Vec<Effect> {
        let panel = self.tabs.menu_for.take();
        let items = match read {
            Ok(items) => items,
            Err(code) => {
                let panel = panel.unwrap_or(self.active);
                self.pane_mut(panel).error = Some(crate::i18n::error_text(code));
                return Vec::new();
            }
        };
        if let Some(editor) = &mut self.tabs.editor {
            editor.reload(items.clone());
        }
        if let Some(panel) = panel {
            self.tabs.popup = Some(Popup::hotlist(panel, &items));
        }
        self.popup_in_view()
    }

    /// The open popup scrolled so its cursor is in view - a long history or hotlist is longer
    /// than its card; nothing when none is open.
    fn popup_in_view(&self) -> Vec<Effect> {
        match &self.tabs.popup {
            Some(popup) => vec![scroll::keep_in_view(
                POPUP_SCROLL_ID,
                popup.cursor,
                popup.rows.len(),
            )],
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "tabs_tests.rs"]
mod tests;
