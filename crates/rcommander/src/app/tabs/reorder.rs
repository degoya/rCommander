//! Dragging a tab to another place in its tab bar (RC-010-24), as in Total Commander: a press on
//! a tab arms the gesture, the pointer going further than [`THRESHOLD`] with the button down
//! starts the drag, and while it lasts a bar between two tabs marks where the tab goes; the
//! release puts it there with `tabs.move`, so MCP and the stored tabs see the new order. A press
//! released without a drag is a click and shows the tab. Escape, the pointer leaving the bar or
//! the window cancel; a locked tab moves like any other. With `tabs.drag_reorder` off a press on
//! a tab shows it at once and nothing is dragged.
//!
//! While a drag lasts each tab says which half of it the pointer is over (`view.rs`), so the
//! place is the drawn one; the window's pointer events (`subscription.rs`) count the threshold
//! and catch a release anywhere. The setting and `keys.context_menu_shortcuts` (the keys beside
//! the menus' entries, `context_menu/keys.rs`) are read here, at the start and on every change.

use iced::{
    Point, Task,
    event::{self, Event},
    mouse, window,
};
use rc_db::Db;
use rc_engine::{
    Command, MoveTabRequest, PanelId, TabId,
    settings::{CONTEXT_MENU_KEYS_KEY, TAB_DRAG_KEY, stored_bool},
};

use super::TabsMessage;
use crate::app::{App, Effect, Message, context_menu::keys};

/// How far the pointer moves, in logical pixels, before a press becomes a drag: a file list's
/// distance (`drag/gesture.rs`), about the system's own.
pub const THRESHOLD: f32 = 5.0;

/// The tab drag's messages.
#[derive(Debug, Clone)]
pub enum DragMsg {
    /// A press on a tab.
    Press(PanelId, TabId),
    /// The pointer moved, in the window's coordinates, while a press may become a drag.
    Moved(Point),
    /// While a drag lasts, the pointer is over a tab of `panel`'s bar: the place before tab
    /// `.1` of the row, or after the last one.
    Over(PanelId, usize),
    /// The left button came up.
    Released,
    /// The pointer left the bar or the window.
    Cancel,
}

/// A press on a tab, and the drag it may become.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabDrag {
    pub panel: PanelId,
    pub tab: TabId,
    /// The pointer's first position after the press.
    origin: Option<Point>,
    /// The pointer went further than the threshold: the tab is being dragged.
    pub dragging: bool,
    /// Where the tab would go: before the row's tab of this index, or the row's length for
    /// after the last; `None` until the pointer was over a tab.
    pub gap: Option<usize>,
}

impl TabDrag {
    pub fn press(panel: PanelId, tab: TabId) -> Self {
        Self {
            panel,
            tab,
            origin: None,
            dragging: false,
            gap: None,
        }
    }

    /// The pointer is at `at`: past the threshold from where it was first seen, the drag starts.
    pub fn moved(&mut self, at: Point) {
        match self.origin {
            None => self.origin = Some(at),
            Some(from) if from.distance(at) > THRESHOLD => self.dragging = true,
            Some(_) => {}
        }
    }

    /// The pointer is over the place `gap` of `panel`'s bar; another bar's place is none of
    /// this drag's.
    pub fn over(&mut self, panel: PanelId, gap: usize) {
        if self.dragging && panel == self.panel {
            self.gap = Some(gap);
        }
    }
}

/// The place of tab `index`, `width` wide, the pointer is over at `x` from its left edge: before
/// it on its left half, after it on its right half.
pub fn gap_at(index: usize, x: f32, width: f32) -> usize {
    if x < width / 2.0 { index } else { index + 1 }
}

/// The place `tabs.move` takes for the tab at `from` dropped at `gap` - a gap counted with the
/// tab still in its row; `None` when the tab stays where it is.
pub fn target(from: usize, gap: usize) -> Option<usize> {
    let to = if gap > from { gap - 1 } else { gap };
    (to != from).then_some(to)
}

/// While a tab is pressed: the pointer's moves, the button's release, the pointer leaving the
/// window.
pub fn pointer_events(event: Event, _status: event::Status, _id: window::Id) -> Option<Message> {
    let message = match event {
        Event::Mouse(mouse::Event::CursorMoved { position }) => DragMsg::Moved(position),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => DragMsg::Released,
        Event::Mouse(mouse::Event::CursorLeft) => DragMsg::Cancel,
        _ => return None,
    };
    Some(wrap(message))
}

fn wrap(message: DragMsg) -> Message {
    Message::Tabs(TabsMessage::Drag(message))
}

/// Whether the setting `key` is one read here.
fn concerns(key: &str) -> bool {
    key == TAB_DRAG_KEY || key == CONTEXT_MENU_KEYS_KEY
}

/// `tabs.drag_reorder` and `keys.context_menu_shortcuts`.
async fn load(db: Db) -> (bool, bool) {
    (
        stored_bool(&db, TAB_DRAG_KEY).await,
        stored_bool(&db, CONTEXT_MENU_KEYS_KEY).await,
    )
}

impl App {
    /// Whether the window follows the pointer for a pressed tab.
    pub(in crate::app) fn tab_pressed(&self) -> bool {
        self.tabs.drag.is_some()
    }

    /// Reads the two settings (the start, and after one changed).
    pub(in crate::app) fn tabs_settings_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(load(db), |(drag, keys)| {
            Message::Tabs(TabsMessage::Settings { drag, keys })
        })
    }

    /// The engine stored the setting `key`: one of the two is read again (RC-010-24).
    pub(in crate::app) fn tabs_setting_changed(&self, key: &str) -> Vec<Effect> {
        if concerns(key) {
            vec![Effect::Task(self.tabs_settings_task())]
        } else {
            Vec::new()
        }
    }

    /// The settings were read.
    pub(super) fn tabs_settings(&mut self, drag: bool, menu_keys: bool) {
        self.tabs.no_drag = !drag;
        if !drag {
            self.tabs.drag = None;
        }
        keys::set_shown(menu_keys);
    }

    pub(super) fn on_tab_drag(&mut self, message: DragMsg) -> Vec<Effect> {
        match message {
            DragMsg::Press(panel, tab) if self.tabs.no_drag => {
                return self.on_tabs(TabsMessage::Select(panel, tab));
            }
            DragMsg::Press(panel, tab) => self.tabs.drag = Some(TabDrag::press(panel, tab)),
            DragMsg::Moved(at) => {
                if let Some(drag) = &mut self.tabs.drag {
                    drag.moved(at);
                }
            }
            DragMsg::Over(panel, gap) => {
                if let Some(drag) = &mut self.tabs.drag {
                    drag.over(panel, gap);
                }
            }
            DragMsg::Released => return self.tab_released(),
            DragMsg::Cancel => self.tabs.drag = None,
        }
        Vec::new()
    }

    /// The button came up: a press without a drag shows its tab, a drag puts the tab at its place.
    fn tab_released(&mut self) -> Vec<Effect> {
        let Some(drag) = self.tabs.drag.take() else {
            return Vec::new();
        };
        if !drag.dragging {
            return self.on_tabs(TabsMessage::Select(drag.panel, drag.tab));
        }
        let from = self.tabs.strips[drag.panel.index()]
            .iter()
            .position(|view| view.id == drag.tab);
        match (from, drag.gap) {
            (Some(from), Some(gap)) => match target(from, gap) {
                Some(to) => self.move_tab(drag.panel, drag.tab, to),
                None => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// Escape while a tab is dragged: the drag is cancelled.
    pub(super) fn tab_drag_key(&mut self, key: &str) -> Vec<Effect> {
        if key == "Escape" {
            self.tabs.drag = None;
        }
        Vec::new()
    }

    /// `tab` of `panel` goes to place `to` of its row.
    pub(super) fn move_tab(&self, panel: PanelId, tab: TabId, to: usize) -> Vec<Effect> {
        let command = Command::MoveTab(MoveTabRequest {
            panel: Some(panel),
            tab,
            to,
        });
        vec![Effect::Dispatch(panel, vec![command])]
    }
}

#[cfg(test)]
#[path = "reorder_tests.rs"]
mod tests;
