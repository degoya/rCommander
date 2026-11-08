//! Column widths and the divider between the panels, dragged with the mouse (RC-010-20), after
//! Total Commander: the gap left of a fixed column's title is that column's grip, the gap between
//! the panels the divider's; the pointer turns into the resize arrows over either. A press grabs,
//! the window follows the pointer's moves until the button is released (or the pointer leaves
//! the window), then stores the new width as a setting. A double-click puts a column back to its
//! default width and the divider to 50:50, as the command palette's "Divider to 50 %" does.
//!
//! The widths are shared by both panels and kept at the default list font size; the file list
//! scales them and gives the name the rest (`file_list/columns.rs`). The divider is the left
//! panel's share in per mille, held so that each panel keeps [`MIN_PANEL`] (also a terminal in a
//! panel, which fits its grid to the new width). Both are ordinary settings (`columns.*`,
//! `panels.split_permille`): MCP reads and sets them with `settings_get` and `settings_set`, and a
//! change from there shows at once. A drag is a gesture of the window, not a command. The
//! separate folder tree's width (`panels.tree_width`, RC-100-88) is dragged at its right edge the
//! same way; one width for both trees, kept at the default list font size too. A column set's
//! widths (RC-100-90) are its own: dragged the same way, kept in the set with
//! `column_sets.update` when the button goes up (`sets.rs`, which also reads the sets).

use iced::{
    Element, Length, Point, Size, Task,
    event::{self, Event},
    mouse,
    widget::{Space, mouse_area},
    window,
};
use rc_db::Db;
use rc_engine::{
    Command,
    settings::{
        OWNER_COLUMN_KEY, PANEL_SPLIT_KEY, SettingKind, TREE_WIDTH_KEY, setting, stored_bool,
        stored_setting,
    },
};
use serde_json::Value;

use super::{
    App, Effect, Message,
    file_list::{Column, ColumnWidths},
};
use crate::theme::fonts;

mod sets;

/// The divider's width: the gap between the panels.
pub const DIVIDER_WIDTH: f32 = 8.0;
/// The narrowest a panel gets by the divider.
pub const MIN_PANEL: f32 = 240.0;

/// What a grip drags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A fixed column's width, by the grip left of its title.
    Column(Column),
    /// The width of the column `index` of the column set `set` (RC-100-90).
    SetColumn { set: i64, index: usize },
    /// The divider between the panels.
    Divider,
    /// The separate folder tree's right edge (RC-100-88).
    Tree,
}

#[derive(Debug, Clone)]
pub enum ResizeMsg {
    /// A press on a grip.
    Grab(Target),
    /// A double-click on a grip: the column's default width, the divider in the middle.
    Reset(Target),
    /// The pointer moved while a grip is held, in the window's coordinates.
    Moved(Point),
    /// The button went up, or the pointer left the window: the drag ends.
    Released,
    /// The panels' area was measured.
    Area(Size),
    /// The stored widths (with the owner column's switch) and the divider's share, read at the
    /// start and after one changed.
    Loaded(ColumnWidths, f32),
    /// The separate tree's stored width, read at the start and after it changed.
    TreeWidth(f32),
    /// The column sets, read at the start and after one changed (RC-100-90).
    Sets(Vec<rc_engine::ColumnSet>),
    /// A right click on a panel's column header: the menu of the column sets.
    Menu(rc_engine::PanelId),
}

/// The widths in force and the drag in progress.
#[derive(Debug)]
pub struct Resize {
    /// The fixed columns' widths, shared by both panels.
    pub columns: ColumnWidths,
    /// The column sets as last read, with the widths dragged since (RC-100-90).
    sets: Vec<rc_engine::ColumnSet>,
    /// The left panel's share of the panels' area, per mille.
    split: f32,
    /// The panels' area's width; 0 until measured.
    area: f32,
    /// The separate tree's width at the default list font size.
    tree: f32,
    grab: Option<Grab>,
}

impl Default for Resize {
    fn default() -> Self {
        Self {
            columns: ColumnWidths::default(),
            sets: Vec::new(),
            split: split_bounds().2,
            area: 0.0,
            tree: tree_bounds().2,
            grab: None,
        }
    }
}

/// A grip held.
#[derive(Debug, Clone, Copy)]
struct Grab {
    target: Target,
    /// The pointer's first position after the press, from which the drag counts.
    from: Option<f32>,
    /// The width or the share when grabbed.
    start: f32,
    moved: bool,
}

impl Resize {
    /// The left panel's share of the panels' area, per mille, held for the area as measured.
    pub fn left_share(&self) -> u16 {
        clamp_split(self.split, self.area)
    }

    /// Whether a grip is held, so the window follows the pointer.
    pub fn grabbing(&self) -> bool {
        self.grab.is_some()
    }

    /// The separate tree's width on screen, scaled with the list font as the columns are.
    pub fn tree_width(&self) -> f32 {
        self.tree * fonts::list().column_scale()
    }

    fn value(&self, target: Target) -> f32 {
        match target {
            Target::Column(column) => self.columns.get(column).unwrap_or_default(),
            Target::SetColumn { set, index } => self.set_width(set, index).unwrap_or_default(),
            Target::Divider => self.split,
            Target::Tree => self.tree,
        }
    }

    fn set_value(&mut self, target: Target, value: f32) {
        match target {
            Target::Column(column) => self.columns.set(column, value),
            Target::SetColumn { set, index } => self.set_set_width(set, index, value),
            Target::Divider => self.split = f32::from(clamp_split(value, self.area)),
            Target::Tree => {
                let (min, max, _) = tree_bounds();
                self.tree = value.clamp(min, max).round();
            }
        }
    }

    /// The pointer at `x`: the held column's left edge or the divider follows it.
    fn drag(&mut self, x: f32) {
        let Some(grab) = self.grab.as_mut() else {
            return;
        };
        let from = *grab.from.get_or_insert(x);
        let moved = x - from;
        grab.moved |= moved != 0.0;
        let grab = *grab;
        let value = match grab.target {
            // The grip is the column's left edge: to the right narrows it.
            Target::Column(_) | Target::SetColumn { .. } => {
                grab.start - moved / fonts::list().column_scale()
            }
            // The grip is the tree's right edge: to the right widens it.
            Target::Tree => grab.start + moved / fonts::list().column_scale(),
            Target::Divider if self.area > DIVIDER_WIDTH => {
                grab.start + moved * 1000.0 / (self.area - DIVIDER_WIDTH)
            }
            Target::Divider => return,
        };
        self.set_value(grab.target, value);
    }
}

/// `permille` held within the divider's bounds and, in an area `area` wide (once measured), so
/// that each panel keeps [`MIN_PANEL`]; the middle in an area too narrow for two of them.
pub fn clamp_split(permille: f32, area: f32) -> u16 {
    let (min, max, middle) = split_bounds();
    let room = area - DIVIDER_WIDTH;
    let (low, high) = if area <= 0.0 {
        (min, max)
    } else if room < 2.0 * MIN_PANEL {
        return middle as u16;
    } else {
        let least = MIN_PANEL / room * 1000.0;
        (min.max(least), max.min(1000.0 - least))
    };
    let permille = if permille.is_finite() {
        permille
    } else {
        middle
    };
    permille.clamp(low, high).round() as u16
}

/// The least, the largest and the default share of the divider's schema row, per mille.
fn split_bounds() -> (f32, f32, f32) {
    match setting(PANEL_SPLIT_KEY).map(|spec| spec.kind) {
        Some(SettingKind::Int { min, max, default }) => (min as f32, max as f32, default as f32),
        _ => (500.0, 500.0, 500.0),
    }
}

/// The least, the largest and the default width of the separate tree's schema row.
fn tree_bounds() -> (f32, f32, f32) {
    match setting(TREE_WIDTH_KEY).map(|spec| spec.kind) {
        Some(SettingKind::Int { min, max, default }) => (min as f32, max as f32, default as f32),
        _ => (220.0, 220.0, 220.0),
    }
}

/// Whether the setting `key` is one of the widths, the owner column's switch, the divider or
/// the separate tree's width.
pub fn concerns(key: &str) -> bool {
    key == PANEL_SPLIT_KEY
        || key == OWNER_COLUMN_KEY
        || key == TREE_WIDTH_KEY
        || Column::FIXED
            .iter()
            .any(|column| column.setting() == Some(key))
}

/// The stored widths with the owner column's switch, and the divider's share; each its default
/// when unset or not valid.
pub async fn load(db: Db) -> (ColumnWidths, f32) {
    let mut columns = ColumnWidths::default();
    for column in Column::FIXED {
        if let Some(key) = column.setting()
            && let Some(width) = number(&db, key).await
        {
            columns.set(column, width as f32);
        }
    }
    columns.owner = stored_bool(&db, OWNER_COLUMN_KEY).await;
    let split = number(&db, PANEL_SPLIT_KEY)
        .await
        .map_or(split_bounds().2, |share| share as f32);
    (columns, split)
}

/// The separate tree's stored width; its default when unset or not valid.
pub async fn load_tree_width(db: Db) -> f32 {
    number(&db, TREE_WIDTH_KEY)
        .await
        .map_or(tree_bounds().2, |width| width as f32)
}

async fn number(db: &Db, key: &str) -> Option<f64> {
    stored_setting(db, key)
        .await
        .and_then(|value| value.as_f64())
}

/// While a grip is held: the pointer's moves, the button's release, the pointer leaving the
/// window.
pub fn pointer_events(event: Event, _status: event::Status, _id: window::Id) -> Option<Message> {
    let message = match event {
        Event::Mouse(mouse::Event::CursorMoved { position }) => ResizeMsg::Moved(position),
        Event::Mouse(
            mouse::Event::ButtonReleased(mouse::Button::Left) | mouse::Event::CursorLeft,
        ) => ResizeMsg::Released,
        _ => return None,
    };
    Some(wrap(message))
}

/// The grip left of a column's title in the header that drags `target`, `width` wide and
/// `height` high.
pub(super) fn column_grip<'a>(target: Target, width: f32, height: f32) -> Element<'a, Message> {
    grip(target, Space::new().width(width).height(height))
}

/// The divider between the panels.
pub(super) fn divider_grip<'a>() -> Element<'a, Message> {
    grip(
        Target::Divider,
        Space::new().width(DIVIDER_WIDTH).height(Length::Fill),
    )
}

/// The separate tree's right edge, as wide as the divider.
pub(super) fn tree_grip<'a>() -> Element<'a, Message> {
    grip(
        Target::Tree,
        Space::new().width(DIVIDER_WIDTH).height(Length::Fill),
    )
}

fn grip<'a>(target: Target, area: Space) -> Element<'a, Message> {
    mouse_area(area)
        .interaction(mouse::Interaction::ResizingHorizontally)
        .on_press(wrap(ResizeMsg::Grab(target)))
        // A click quicker than the pointer's subscription still ends here.
        .on_release(wrap(ResizeMsg::Released))
        .on_double_click(wrap(ResizeMsg::Reset(target)))
        .into()
}

fn wrap(message: ResizeMsg) -> Message {
    Message::Resize(message)
}

impl App {
    pub(super) fn on_resize(&mut self, message: ResizeMsg) -> Vec<Effect> {
        let resize = &mut self.resize;
        match message {
            ResizeMsg::Grab(target) => {
                resize.grab = Some(Grab {
                    target,
                    from: None,
                    start: resize.value(target),
                    moved: false,
                });
            }
            ResizeMsg::Reset(target) => {
                resize.grab = None;
                let value = match target {
                    Target::Column(column) => column.bounds().2,
                    Target::SetColumn { set, index } => resize.default_width(set, index),
                    Target::Divider => split_bounds().2,
                    Target::Tree => tree_bounds().2,
                };
                resize.set_value(target, value);
                return self.store_size(target);
            }
            ResizeMsg::Moved(at) => resize.drag(at.x),
            ResizeMsg::Released => {
                if let Some(grab) = resize.grab.take()
                    && grab.moved
                {
                    return self.store_size(grab.target);
                }
            }
            ResizeMsg::Area(size) => resize.area = size.width,
            ResizeMsg::Loaded(columns, split) => {
                resize.columns = columns;
                resize.split = split;
            }
            ResizeMsg::TreeWidth(width) => resize.set_value(Target::Tree, width),
            // While a set's column is dragged, its width stays the dragged one.
            ResizeMsg::Sets(sets) => {
                self.columns_page_read(&sets);
                if self.resize.grab.is_none() {
                    self.resize.sets = sets;
                }
            }
            ResizeMsg::Menu(panel) => return self.column_sets_menu(panel),
        }
        Vec::new()
    }

    /// Stores the width or the share of `target` as it is now; MCP sees it with `settings_get`.
    fn store_size(&self, target: Target) -> Vec<Effect> {
        let (key, value) = match target {
            Target::Column(column) => (column.setting(), self.resize.columns.get(column)),
            Target::SetColumn { set, .. } => return self.store_set(set),
            Target::Divider => (Some(PANEL_SPLIT_KEY), Some(self.resize.split)),
            Target::Tree => (Some(TREE_WIDTH_KEY), Some(self.resize.tree)),
        };
        let (Some(key), Some(value)) = (key, value) else {
            return Vec::new();
        };
        let set = Command::SetSetting {
            key: key.to_owned(),
            value: Value::from(value.round() as i64),
        };
        vec![Effect::Dispatch(self.active, vec![set])]
    }

    /// Reads the stored widths, the divider and the separate tree's width (the start, and after
    /// one changed), and the column sets.
    pub(super) fn resize_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        let tree = Task::perform(load_tree_width(db.clone()), |width| {
            wrap(ResizeMsg::TreeWidth(width))
        });
        let widths = Task::perform(load(db), |(columns, split)| {
            wrap(ResizeMsg::Loaded(columns, split))
        });
        Task::batch([widths, tree, self.column_sets_task()])
    }
}

#[cfg(test)]
#[path = "resize_tests.rs"]
mod tests;
