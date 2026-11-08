//! The diff window's Structure view (RC-100-41): when a compare plugin claims both files - JSON,
//! CSV - it compares them by structure beside the line diff (`Engine::structure_paths`), and the
//! window offers a second view: the plugin's changes as a table - path, column, kind, old and new
//! value - filtered by kind, a click or Enter on one jumping to its place in both sides of the
//! text view. Without a plugin, without a licence, for a key file, the view is not offered and
//! the line diff is all there is; over `plugins.compare_limit_mib` or when the plugin failed, a
//! note says why.

use iced::Size;
use rc_engine::{
    CancellationToken, Chord, Engine, VfsPath,
    compare::structure::{ChangeKind, Comparison, StructureOutcome},
};

use super::{CompareMsg, DiffWindow};
use crate::app::{App, Effect, list_window::ListWindow};

/// Which view the diff window shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DiffView {
    /// The line diff, side by side.
    #[default]
    Text,
    /// A compare plugin's changes.
    Structure,
}

/// What became of the structural comparison of the window's two files.
#[derive(Debug, Default)]
pub enum Structure {
    /// Being compared.
    #[default]
    Running,
    /// A plugin compared them: the Structure view is offered.
    Compared(Box<StructurePane>),
    /// Nothing is offered; the note says why when the person should know.
    Absent(Option<StructureNote>),
}

/// Why a structural comparison the person may expect did not come.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructureNote {
    /// Either file is larger than `plugins.compare_limit_mib`.
    TooLarge(u64),
    /// The plugin failed with this code.
    Failed { plugin: String, code: String },
}

/// A compare plugin's changes as the window shows them.
#[derive(Debug)]
pub struct StructurePane {
    pub plugin: String,
    pub comparison: Comparison,
    /// The notices in the plugin's words.
    pub notices: Vec<String>,
    /// The kinds the filter hides.
    pub hidden: Vec<ChangeKind>,
    /// The cursor among the changes shown, the first row on screen and the rows that fit.
    pub rows: ListWindow,
}

impl StructurePane {
    fn new(plugin: String, comparison: Comparison, notices: Vec<String>) -> Self {
        Self {
            plugin,
            comparison,
            notices,
            hidden: Vec::new(),
            rows: ListWindow::new(1),
        }
    }

    /// The indexes of the changes the filter lets through, in the plugin's order.
    pub fn shown(&self) -> Vec<usize> {
        self.comparison
            .changes
            .iter()
            .enumerate()
            .filter(|(_, change)| !self.hidden.contains(&change.kind))
            .map(|(index, _)| index)
            .collect()
    }

    /// Shows or hides the changes of `kind`; the cursor goes back to the first.
    pub fn filter(&mut self, kind: ChangeKind, shown: bool) {
        self.hidden.retain(|hidden| *hidden != kind);
        if !shown {
            self.hidden.push(kind);
        }
        self.rows.reset();
    }
}

#[derive(Debug, Clone)]
pub enum StructureMsg {
    /// The structural comparison of the window's comparison numbered so.
    Loaded(u64, Result<StructureOutcome, &'static str>),
    View(DiffView),
    /// A kind shown (`true`) or hidden by the filter.
    Filter(ChangeKind, bool),
    /// A change, by its place among all, opened in the text view.
    Jump(usize),
    Scroll(isize),
    Viewport(Size),
}

/// The structural comparison, off the window's thread.
pub(super) async fn load(
    engine: Engine,
    left: VfsPath,
    right: VfsPath,
    cancel: CancellationToken,
) -> Result<StructureOutcome, &'static str> {
    engine
        .structure_paths(&left, &right, &cancel)
        .await
        .map_err(|error| error.code())
}

impl DiffWindow {
    /// The structural comparison's answer: the view is offered with a comparison, else the text
    /// view is shown.
    pub fn structure_loaded(&mut self, result: Result<StructureOutcome, &'static str>) {
        self.structure = match result {
            Ok(StructureOutcome::Compared {
                plugin,
                comparison,
                notices,
                ..
            }) => Structure::Compared(Box::new(StructurePane::new(plugin, comparison, notices))),
            Ok(StructureOutcome::TooLarge { limit_mib }) => {
                Structure::Absent(Some(StructureNote::TooLarge(limit_mib)))
            }
            Ok(StructureOutcome::Failed { plugin, code }) => {
                Structure::Absent(Some(StructureNote::Failed { plugin, code }))
            }
            // No plugin, no licence, a key file, or the files could not be read: the line diff
            // says what there is to say.
            Ok(_) | Err(_) => Structure::Absent(None),
        };
        if self.pane().is_none() {
            self.view = DiffView::Text;
        }
    }

    /// The plugin's changes, when it compared the files.
    pub fn pane(&self) -> Option<&StructurePane> {
        match &self.structure {
            Structure::Compared(pane) => Some(&**pane),
            _ => None,
        }
    }

    fn pane_mut(&mut self) -> Option<&mut StructurePane> {
        match &mut self.structure {
            Structure::Compared(pane) => Some(&mut **pane),
            _ => None,
        }
    }

    /// Whether the Structure view is on screen.
    pub fn showing_structure(&self) -> bool {
        self.view == DiffView::Structure && self.pane().is_some()
    }

    /// Change `index` in the text view: the first row holding its old line, else its new line,
    /// jumped to as a difference is. `false` when the text view has no such line.
    pub fn jump_to_change(&mut self, index: usize) -> bool {
        let Some(change) = self
            .pane()
            .and_then(|pane| pane.comparison.changes.get(index))
        else {
            return false;
        };
        let line =
            |number: Option<u32>| number.and_then(|n| usize::try_from(n).ok()?.checked_sub(1));
        let (old, new) = (line(change.old_line), line(change.new_line));
        let row = self.diff().and_then(|diff| {
            let by = |wanted: Option<usize>, left: bool| {
                wanted.and_then(|wanted| {
                    diff.rows.iter().position(|row| {
                        if left {
                            row.left == Some(wanted)
                        } else {
                            row.right == Some(wanted)
                        }
                    })
                })
            };
            by(old, true).or_else(|| by(new, false))
        });
        let Some(row) = row else {
            return false;
        };
        self.view = DiffView::Text;
        self.go_to(Some(row))
    }
}

impl App {
    pub(super) fn on_structure(&mut self, message: StructureMsg) -> Vec<Effect> {
        let Some(window) = self.compare.diff.as_mut() else {
            return Vec::new();
        };
        match message {
            StructureMsg::Loaded(generation, result) => {
                if window.generation == generation {
                    window.structure_loaded(result);
                }
            }
            StructureMsg::View(view) => {
                window.view = if window.pane().is_some() {
                    view
                } else {
                    DiffView::Text
                };
            }
            StructureMsg::Filter(kind, shown) => {
                if let Some(pane) = window.pane_mut() {
                    pane.filter(kind, shown);
                }
            }
            StructureMsg::Jump(index) => {
                window.jump_to_change(index);
            }
            StructureMsg::Scroll(rows) => {
                if let Some(pane) = window.pane_mut() {
                    let len = pane.shown().len();
                    pane.rows.scroll(rows, len);
                }
            }
            StructureMsg::Viewport(size) => {
                if let Some(pane) = window.pane_mut() {
                    let len = pane.shown().len();
                    pane.rows
                        .set_page(super::structure_view::rows_for(size.height), len);
                }
            }
        }
        Vec::new()
    }

    /// A key of the Structure view: the arrows, Page Up and Down, Home and End move the cursor
    /// among the changes, Enter opens the one under it in the text view. `None` outside the
    /// view, during a question or a field, and for every other key.
    pub(super) fn structure_key(&mut self, chord: &Chord, captured: bool) -> Option<Vec<Effect>> {
        let window = self.compare.diff.as_mut()?;
        if !window.showing_structure() || window.ask.is_some() || window.field.is_some() {
            return None;
        }
        if chord.ctrl || chord.alt || chord.shift {
            return None;
        }
        let pane = window.pane_mut()?;
        let shown = pane.shown();
        let len = shown.len();
        let page = pane.rows.page() as isize;
        match chord.key.as_str() {
            "ArrowDown" => pane.rows.step(1, len),
            "ArrowUp" => pane.rows.step(-1, len),
            "PageDown" => pane.rows.step(page, len),
            "PageUp" => pane.rows.step(-page, len),
            "Home" => pane.rows.point_at(0, len),
            "End" => pane.rows.point_at(len.saturating_sub(1), len),
            "Enter" if !captured => {
                if let Some(index) = shown.get(pane.rows.cursor()).copied() {
                    window.jump_to_change(index);
                }
            }
            _ => return None,
        }
        Some(Vec::new())
    }
}

/// A message of the Structure view.
pub(super) fn wrap(message: StructureMsg) -> crate::app::Message {
    super::wrap(CompareMsg::Structure(message))
}

#[cfg(test)]
#[path = "structure_tests.rs"]
mod tests;
