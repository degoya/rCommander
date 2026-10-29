//! Marking rows with the mouse as Windows Explorer does (RC-010-02, owner 2026-10-18): Ctrl+click
//! marks or unmarks one row, Shift+click marks the rows from the anchor to the clicked one in
//! place of the marks before, Ctrl+Shift+click adds them to the marks. The anchor is the row of
//! the last plain or Ctrl click in the folder, else the cursor's. Either way the cursor goes to
//! the clicked row and the `..` row is never marked; the brief view counts its rows in the same
//! order as it draws them.
//!
//! A plain click moves the cursor only, as in Total Commander; with `mouse.click_clears_marks`
//! on it clears the marks too, as Explorer does - after a press on a marked row only once the
//! press ends without a drag, so the marked rows can still be dragged. A click with Ctrl or
//! Shift starts no drag (`drag.rs`). The marks stay the engine's: the clicks send
//! `panel.toggle_selection` and `panel.deselect_all`, so MCP sees what the mouse marked.

use iced::Task;
use rc_engine::{
    Command, EngineEvent, PanelId, VfsPath,
    settings::{CLICK_CLEARS_MARKS_KEY, stored_bool},
};

use super::{DragMsg, gesture::Gesture, wrap};
use crate::app::{App, Effect, Message, panel::Pane};

/// What the clicks remember: the setting, and per panel where a range starts.
#[derive(Debug, Default)]
pub struct ClickMarks {
    /// `mouse.click_clears_marks`: a plain click clears the marks.
    clears: bool,
    /// Where a Shift+click's range starts, by [`PanelId::index`].
    anchors: [Option<Anchor>; 2],
    /// A plain press on a marked row with `clears` on: the panel whose marks go when the press
    /// ends without a drag.
    clear_on_release: Option<PanelId>,
}

/// The row a range starts from, by name, so a reread or another order keeps it.
#[derive(Debug, Clone, PartialEq)]
struct Anchor {
    /// The folder of the click; in another folder the cursor is the anchor.
    folder: VfsPath,
    /// The entry clicked; `None` for the `..` row.
    name: Option<String>,
}

impl Anchor {
    fn at(pane: &Pane, row: usize) -> Self {
        Self {
            folder: pane.state.path.clone(),
            name: entry_name(pane, row),
        }
    }

    /// The anchor's row in `pane`; `None` in another folder or when its entry is gone.
    fn row(&self, pane: &Pane) -> Option<usize> {
        if self.folder != pane.state.path {
            return None;
        }
        let Some(name) = &self.name else {
            return Some(0);
        };
        let parent = usize::from(pane.has_parent_row());
        pane.entries()
            .iter()
            .position(|entry| entry.name == *name)
            .map(|index| index + parent)
    }
}

/// Whether `event` changed `mouse.click_clears_marks`.
pub fn concerns(event: &EngineEvent) -> bool {
    matches!(event, EngineEvent::SettingChanged { key } if key == CLICK_CLEARS_MARKS_KEY)
}

impl App {
    /// The gesture a press on `row` starts: a drag may follow a plain press, a press with Ctrl or
    /// Shift only marks.
    pub(super) fn pressed_on(&self, panel: PanelId, row: usize) -> Gesture {
        let held = self.modifiers_held();
        if held.ctrl || held.shift {
            Gesture::Idle
        } else {
            Gesture::press(panel, row)
        }
    }

    /// A click on `row` of `panel`: the cursor goes there ([`App::pointing`]), and the marks
    /// change as Ctrl and Shift say.
    pub(in crate::app) fn click(&mut self, panel: PanelId, row: usize) -> Effect {
        let marks = self.click_marks(panel, row);
        let mut effect = self.pointing(panel, row, false);
        if let Effect::Dispatch(_, commands) = &mut effect {
            commands.extend(marks);
        }
        effect
    }

    /// The commands that change the marks for a click on `row`, and the anchor it leaves.
    fn click_marks(&mut self, panel: PanelId, row: usize) -> Vec<Command> {
        let held = self.modifiers_held();
        let pane = &self.panes[panel.index()];
        let marks = &mut self.drag.marks;
        marks.clear_on_release = None;
        let anchor = &mut marks.anchors[panel.index()];
        let id = Some(panel);
        if held.shift {
            let from = match anchor.as_ref().and_then(|anchor| anchor.row(pane)) {
                Some(from) => from,
                None => {
                    let from = pane.cursor_row();
                    *anchor = Some(Anchor::at(pane, from));
                    from
                }
            };
            let mut names = range_names(pane, from, row);
            let mut commands = Vec::with_capacity(2);
            if held.ctrl {
                names.retain(|name| !pane.state.selection.contains(name));
            } else {
                commands.push(Command::DeselectAll { panel: id });
            }
            if !names.is_empty() {
                commands.push(Command::ToggleSelection { panel: id, names });
            }
            return commands;
        }
        *anchor = Some(Anchor::at(pane, row));
        let name = entry_name(pane, row);
        if held.ctrl {
            return name
                .map(|name| {
                    vec![Command::ToggleSelection {
                        panel: id,
                        names: vec![name],
                    }]
                })
                .unwrap_or_default();
        }
        if !marks.clears || pane.state.selection.is_empty() {
            return Vec::new();
        }
        if name.is_some_and(|name| pane.state.selection.contains(&name)) {
            marks.clear_on_release = Some(panel);
            return Vec::new();
        }
        vec![Command::DeselectAll { panel: id }]
    }

    /// The press ended without a drag: a plain click on a marked row with
    /// `mouse.click_clears_marks` on clears the marks now.
    pub(super) fn click_released(&mut self) -> Vec<Effect> {
        match self.drag.marks.clear_on_release.take() {
            Some(panel) => {
                let clear = Command::DeselectAll { panel: Some(panel) };
                vec![Effect::Dispatch(panel, vec![clear])]
            }
            None => Vec::new(),
        }
    }

    /// The stored `mouse.click_clears_marks`, read.
    pub(super) fn set_click_clears(&mut self, on: bool) {
        self.drag.marks.clears = on;
    }

    /// Reads `mouse.click_clears_marks` (the start, and after it changed).
    pub(in crate::app) fn click_clears_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(
            async move { stored_bool(&db, CLICK_CLEARS_MARKS_KEY).await },
            |on| wrap(DragMsg::ClickClears(on)),
        )
    }
}

/// The name of the entry on `row`; `None` for the `..` row.
fn entry_name(pane: &Pane, row: usize) -> Option<String> {
    pane.entry_index(row)
        .and_then(|index| pane.entries().get(index))
        .map(|entry| entry.name.clone())
}

/// The names on the rows from `a` to `b`, either way round; the `..` row has none.
fn range_names(pane: &Pane, a: usize, b: usize) -> Vec<String> {
    let parent = usize::from(pane.has_parent_row());
    let Some(last) = a.max(b).checked_sub(parent) else {
        return Vec::new();
    };
    let first = a.min(b).saturating_sub(parent);
    pane.entries()
        .iter()
        .take(last + 1)
        .skip(first)
        .map(|entry| entry.name.clone())
        .collect()
}

#[cfg(test)]
#[path = "marks_tests.rs"]
mod tests;
