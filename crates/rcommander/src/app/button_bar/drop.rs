//! Files dropped on a bar (RC-100-87), as Total Commander takes them: on a button that starts a
//! program they are its further arguments (`button_bar.run` with `files`, quoted and checked by
//! the engine as the placeholders' values); with Shift held, a drop on a bar makes a new button of
//! the first file - after the button it lands on, else at the bar's end. Only local paths go: a
//! server's or an archive's entry is no file the program could open, which the status line says;
//! a button with internal rows takes no files (TC's drop on "F8 Delete" stays out).
//!
//! The drop arrives without a place in the window (`drag.rs` waits for it to settle and asks the
//! system where the pointer is, as a share of the window's width and height): the bars' buttons
//! are measured ([`locate`]) and the one under the pointer takes the drop. A drop beside every
//! bar goes on to the panels as before (`DragMsg::Landed`).

use std::path::PathBuf;

use iced::{
    Point, Rectangle, Task,
    advanced::widget::{Id, Operation, operate, operation::Outcome},
    window,
};
use rc_engine::{
    BarButton, BarItem, BarPlace, ButtonAction, CommandKind, CommandRef, PanelView,
    button_bars::MAX_BUTTONS,
};

use super::{App, BbMsg, Effect, view, wrap};
use crate::{app::drag::DragMsg, i18n::t, platform::drag_out::Pointer};

/// What lies under the pointer: a bar, or one of its items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub place: BarPlace,
    /// The item's place in the bar; `None` for the bar beside its items.
    pub item: Option<usize>,
}

/// The bounds of the container `id`, as [`BbMsg::Anchored`].
pub fn measure(id: Id) -> Task<crate::app::Message> {
    operate(Find {
        wanted: vec![(id, None)],
        at: None,
        found: None,
        bounds: None,
    })
    .map(|(_, bounds)| wrap(BbMsg::Anchored(bounds)))
}

/// The target under `pointer` among `wanted` (each container id with what it is).
fn locate(wanted: Vec<(Id, Option<Target>)>, pointer: Pointer) -> Task<Option<Target>> {
    window::oldest().and_then(window::size).then(move |size| {
        let at = Point::new(pointer.across * size.width, pointer.down * size.height);
        operate(Find {
            wanted: wanted.clone(),
            at: Some(at),
            found: None,
            bounds: None,
        })
        .map(|(found, _)| found)
    })
}

/// A widget operation that finds the containers of `wanted`: with `at`, the one there (an item
/// before its bar); without, the bounds of the first.
struct Find {
    wanted: Vec<(Id, Option<Target>)>,
    at: Option<Point>,
    found: Option<Target>,
    bounds: Option<Rectangle>,
}

impl Operation<(Option<Target>, Option<Rectangle>)> for Find {
    fn traverse(
        &mut self,
        operate: &mut dyn FnMut(&mut dyn Operation<(Option<Target>, Option<Rectangle>)>),
    ) {
        operate(self);
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        let Some(id) = id else {
            return;
        };
        let Some((_, target)) = self.wanted.iter().find(|(wanted, _)| wanted == id) else {
            return;
        };
        match self.at {
            None => {
                if self.bounds.is_none() {
                    self.bounds = Some(bounds);
                }
            }
            Some(at) if bounds.contains(at) => {
                // An item lies inside its bar: it wins over the bar, whichever comes first.
                let better = match (self.found, target) {
                    (None, _) => true,
                    (Some(found), Some(target)) => found.item.is_none() && target.item.is_some(),
                    (Some(_), None) => false,
                };
                if better {
                    self.found = *target;
                }
            }
            Some(_) => {}
        }
    }

    fn finish(&self) -> Outcome<(Option<Target>, Option<Rectangle>)> {
        Outcome::Some((self.found, self.bounds))
    }
}

/// The paths of a drop as a button's arguments: absolute local paths, as text.
fn local_paths(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .filter(|path| path.is_absolute())
        .filter_map(|path| path.to_str().map(str::to_owned))
        .collect()
}

/// The new button a file makes: a program's button that starts it, its name the tooltip.
fn button_for(path: &str) -> BarButton {
    let tooltip = std::path::Path::new(path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    BarButton {
        action: ButtonAction::Command(CommandRef {
            kind: CommandKind::Program,
            command: path.to_owned(),
            ..CommandRef::default()
        }),
        icon: None,
        tooltip,
    }
}

impl App {
    /// A drop with the pointer at `pointer`: measured against the bars when one shows, the
    /// answer [`BbMsg::Dropped`]; `None` while no bar shows or the system names no place.
    pub(in crate::app) fn button_drop(
        &mut self,
        paths: &[PathBuf],
        pointer: Option<Pointer>,
    ) -> Option<Vec<Effect>> {
        let pointer = pointer?;
        let mut wanted = Vec::new();
        for place in [BarPlace::Horizontal, BarPlace::Vertical] {
            let Some(bar) = self.button_bar.shown(place) else {
                continue;
            };
            wanted.push((view::bar_id(place), Some(Target { place, item: None })));
            for (index, item) in bar.spec.items.iter().enumerate() {
                if item.button().is_some() {
                    let target = Target {
                        place,
                        item: Some(index),
                    };
                    wanted.push((view::item_id(place, index), Some(target)));
                }
            }
        }
        if wanted.is_empty() {
            return None;
        }
        let paths = paths.to_vec();
        let shift = pointer.shift;
        let task = locate(wanted, pointer).map(move |target| {
            wrap(BbMsg::Dropped {
                paths: paths.clone(),
                target,
                shift,
                pointer: Some(pointer),
            })
        });
        Some(vec![Effect::Task(task)])
    }

    /// A drop measured: on a button its run with the files, with Shift a new button; beside the
    /// bars on to the panels.
    pub(super) fn dropped_on_bar(
        &mut self,
        paths: Vec<PathBuf>,
        target: Option<Target>,
        shift: bool,
        pointer: Option<Pointer>,
    ) -> Vec<Effect> {
        let Some(target) = target else {
            return vec![Effect::Task(Task::done(crate::app::Message::Drag(
                DragMsg::Landed { paths, pointer },
            )))];
        };
        // A drag of the panel's own entries ends here, not on a panel.
        self.forget_own_drag();
        let files = local_paths(&paths);
        if files.len() < paths.len() || files.is_empty() {
            self.info = Some(t!("button_bar.drop_local_only").to_owned());
            if files.is_empty() {
                return Vec::new();
            }
        }
        if shift {
            return self.new_button_of(target, &files[0]);
        }
        let Some(index) = target.item else {
            return Vec::new();
        };
        let Some(bar) = self.button_bar.shown(target.place) else {
            return Vec::new();
        };
        match bar.spec.items.get(index).and_then(BarItem::button) {
            Some(BarButton {
                action: ButtonAction::Command(action),
                ..
            }) if action.kind != CommandKind::Internal => {
                let id = bar.id;
                vec![self.run_button(id, index, files)]
            }
            Some(_) => {
                self.info = Some(t!("button_bar.drop_no_files").to_owned());
                Vec::new()
            }
            None => Vec::new(),
        }
    }

    /// Shift and a drop: a new button that starts `file`, after the item it landed on or at the
    /// bar's end; the bar saved with it.
    fn new_button_of(&mut self, target: Target, file: &str) -> Vec<Effect> {
        let Some(bar) = self.button_bar.shown(target.place) else {
            return Vec::new();
        };
        if bar.spec.items.len() >= MAX_BUTTONS {
            self.info = Some(t!("button_bar.full").to_owned());
            return Vec::new();
        }
        let mut spec = bar.spec.clone();
        let at = target.item.map_or(spec.items.len(), |item| item + 1);
        spec.items.insert(at, BarItem::Button(button_for(file)));
        let (id, name) = (bar.id, bar.name.clone());
        // The panel that shows files takes the answer; a terminal's would only lose it.
        let panel = if self.pane(self.active).state.view == PanelView::Files {
            self.active
        } else {
            self.active.other()
        };
        vec![Effect::Dispatch(
            panel,
            vec![rc_engine::Command::SaveButtonBar {
                id: Some(id),
                name,
                spec,
            }],
        )]
    }
}

#[cfg(test)]
#[path = "drop_tests.rs"]
mod tests;
