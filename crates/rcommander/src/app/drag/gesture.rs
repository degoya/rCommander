//! The drag gesture in a file list, without a window: a press on a row arms it, the pointer
//! going further than [`THRESHOLD`] with the button still down starts the drag, the release ends
//! it. Which entries a drag takes is [`dragged_names`].

use iced::Point;
use rc_engine::PanelId;

use super::super::panel::Pane;

/// How far the pointer moves, in logical pixels, before a press becomes a drag; about the
/// system's own distance (`SM_CXDRAG`), so a click that wobbles stays a click.
pub const THRESHOLD: f32 = 5.0;

/// Where a gesture stands.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Gesture {
    /// No button down on a row.
    #[default]
    Idle,
    /// A row was pressed; `origin` is the pointer's first position seen since.
    Armed {
        panel: PanelId,
        row: usize,
        origin: Option<Point>,
    },
    /// The pointer went further than the threshold: entries are being dragged from `panel`.
    Dragging { panel: PanelId },
}

impl Gesture {
    /// A press on `row` of `panel`.
    pub fn press(panel: PanelId, row: usize) -> Self {
        Self::Armed {
            panel,
            row,
            origin: None,
        }
    }

    /// Whether the pointer's moves and the button's release matter now.
    pub fn tracking(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    /// The pointer is at `at` (window coordinates). When this move starts the drag, the panel
    /// and the row that was pressed.
    pub fn moved(&mut self, at: Point) -> Option<(PanelId, usize)> {
        let Self::Armed { panel, row, origin } = *self else {
            return None;
        };
        match origin {
            None => {
                *self = Self::Armed {
                    panel,
                    row,
                    origin: Some(at),
                };
                None
            }
            Some(from) if from.distance(at) > THRESHOLD => {
                *self = Self::Dragging { panel };
                Some((panel, row))
            }
            Some(_) => None,
        }
    }

    /// The button came up: the panel a drag was going on from, if one was.
    pub fn release(&mut self) -> Option<PanelId> {
        match std::mem::take(self) {
            Self::Dragging { panel } => Some(panel),
            _ => None,
        }
    }

    /// Another press: a drag still going on missed its release (the button came up before the
    /// moves were followed) and is over.
    pub fn pressed(&mut self) {
        if matches!(self, Self::Dragging { .. }) {
            *self = Self::Idle;
        }
    }
}

/// The entries a drag from `row` takes: the selection when the row's entry is selected, else that
/// entry alone; none on the `..` row.
pub fn dragged_names(pane: &Pane, row: usize) -> Vec<String> {
    let Some(entry) = pane
        .entry_index(row)
        .and_then(|index| pane.entries().get(index))
    else {
        return Vec::new();
    };
    if !pane.state.selection.contains(&entry.name) {
        return vec![entry.name.clone()];
    }
    pane.chosen_names()
}

#[cfg(test)]
#[path = "gesture_tests.rs"]
mod tests;
