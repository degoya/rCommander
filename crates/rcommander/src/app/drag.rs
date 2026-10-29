//! Drag and drop (RC-080-01), after Total Commander: entries dragged from one panel onto the
//! other open the copy dialog of F5 (with Shift the move dialog of F6) for them; dragged out of
//! the window they go to the Explorer, a mail or a browser (Windows, `platform::drag_out`);
//! files the Explorer drops onto a panel are copied into its folder (with Shift moved) by the
//! operations queue, with its progress window and conflict questions. Ctrl+C, Ctrl+X and Ctrl+V
//! put files on the clipboard and take them from it the same way (`drag/clipboard.rs`).
//!
//! The gesture (`drag/gesture.rs`) starts with a press on a row; while it lasts the window
//! follows the pointer's moves, the button's release and the pointer leaving the window
//! ([`pointer_events`]). Remote entries are fetched into a temporary folder before they leave
//! the window (`drag/fetch.rs`, `drag/temp.rs`); above a size the person is asked first
//! (`drag/view.rs`). Dropped and pasted paths are hostile input (`drag/dropped.rs`); a drop lands
//! in the panel on its side of the divider, or in the maximized one.
//! A drag or a drop is a gesture of the window, not a command: MCP copies with `ops_copy`.
//! A click with Ctrl or Shift marks rows instead and starts no drag (`drag/marks.rs`).

mod clipboard;
mod dropped;
mod fetch;
mod gesture;
mod marks;
mod temp;
mod view;

use std::{collections::HashMap, path::PathBuf, time::Duration};

use iced::{
    Point, Task,
    event::{self, Event},
    mouse, window,
};
use rc_engine::{Command, FileDialog, PanelId, PanelView};

use self::{
    fetch::{Fetch, FetchKey, Purpose},
    gesture::Gesture,
    temp::TempArea,
};
use super::{
    App, Effect, Message,
    file_ops::{FollowUp, Submit},
};
use crate::{
    i18n::t,
    platform::{
        clipboard_files::ClipboardFiles,
        drag_out::{self, DragOutcome, Pointer},
    },
};

/// How long the window waits after a dropped file for the rest of the drop: the system hands a
/// drop over one file at a time.
const SETTLE: Duration = Duration::from_millis(50);

/// The drag and drop part of the window.
#[derive(Debug, Default)]
pub struct DragUi {
    gesture: Gesture,
    /// The entries the drag in progress takes.
    names: Vec<String>,
    /// Files dropped onto the window, gathered until the drop settles.
    dropped: Vec<PathBuf>,
    /// The fetch waiting for the person's answer (above the size limit).
    question: Option<Fetch>,
    /// The fetch waiting for the session folder, or running as a job.
    running: Option<Fetch>,
    /// Fetched copies by the listing they were fetched from, for the next drag of the same.
    fetched: HashMap<FetchKey, PathBuf>,
    temp: TempArea,
    /// The marks a click with Ctrl or Shift sets, and the anchor of a range.
    marks: marks::ClickMarks,
}

impl DragUi {
    /// Whether the pointer's moves are followed: from a press on a row until its release.
    pub fn tracking(&self) -> bool {
        self.gesture.tracking()
    }

    /// Whether the size question of a fetch waits for the person.
    pub fn asking(&self) -> bool {
        self.question.is_some()
    }
}

#[derive(Debug, Clone)]
pub enum DragMsg {
    /// The pointer moved, in window coordinates, while a press may become a drag.
    Moved(Point),
    /// The left button came up.
    Released,
    /// The pointer left the window.
    Left,
    /// A drag out of the window ended; `fetched` is the temporary folder it came from.
    Ended {
        panel: PanelId,
        outcome: DragOutcome,
        fetched: Option<PathBuf>,
    },
    /// Another program dropped a file onto the window; more of the same drop may follow.
    FileDropped(PathBuf),
    /// The drop is complete; where the pointer stands, where the system says.
    Settled(Option<Pointer>),
    /// The answer to the size question: fetch, or not.
    Fetch(bool),
    /// The session's temporary folder was made, or could not be.
    Session(Result<PathBuf, String>),
    /// Files went to the clipboard, or did not.
    Copied { count: usize, cut: bool, ok: bool },
    /// What Ctrl+V found on the clipboard.
    Pasted(Option<ClipboardFiles>),
    /// The stored `mouse.click_clears_marks` (`drag/marks.rs`).
    ClickClears(bool),
}

fn wrap(message: DragMsg) -> Message {
    Message::Drag(message)
}

/// The pointer's events while a gesture lasts (`subscription.rs` listens only then).
pub fn pointer_events(event: Event, _status: event::Status, _id: window::Id) -> Option<Message> {
    let message = match event {
        Event::Mouse(mouse::Event::CursorMoved { position }) => DragMsg::Moved(position),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => DragMsg::Released,
        Event::Mouse(mouse::Event::CursorLeft) => DragMsg::Left,
        _ => return None,
    };
    Some(wrap(message))
}

/// A file dropped onto the window, as the window's events hand it over.
pub fn file_dropped(path: PathBuf) -> Message {
    wrap(DragMsg::FileDropped(path))
}

impl App {
    /// Sees every message first: a press on a row arms the gesture, another press ends one that
    /// missed its release, the app's end removes the fetched copies. While the size question is
    /// open it has the keys and the clicks.
    pub(super) fn drag_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        match message {
            Message::Click(panel, row) if self.drag.question.is_none() => {
                self.drag.gesture = self.pressed_on(*panel, *row);
            }
            Message::Engine(event) if marks::concerns(event) => {
                return Some(vec![Effect::Task(self.click_clears_task())]);
            }
            Message::Press => {
                self.drag.gesture.pressed();
                if self.drag.question.is_some() {
                    return Some(Vec::new());
                }
            }
            // The fetched copies go first, off the window's thread; the app ends on the
            // `Saved` that follows, which finds no session folder any more.
            Message::Saved => {
                if let Some(folder) = self.drag.temp.take_session() {
                    let removed =
                        Task::perform(temp::remove_off_thread(folder), |()| Message::Saved);
                    return Some(vec![Effect::Task(removed)]);
                }
            }
            // Tab walks the focus ring (RC-080-16).
            Message::Key(chord)
                if self.drag.question.is_some() && super::field_walk::tab(chord).is_some() =>
            {
                return super::field_walk::walk_key(chord);
            }
            // A key a stop of the ring took is not the question's again.
            Message::KeyCaptured(chord)
                if self.drag.question.is_some() && super::field_walk::taken(chord) =>
            {
                return Some(Vec::new());
            }
            Message::Key(chord) | Message::KeyCaptured(chord) if self.drag.question.is_some() => {
                let plain = chord.is_plain();
                return Some(match chord.key.as_str() {
                    "Escape" if plain => self.on_drag(DragMsg::Fetch(false)),
                    "Enter" if plain => self.on_drag(DragMsg::Fetch(true)),
                    _ => Vec::new(),
                });
            }
            _ => {}
        }
        None
    }

    pub(super) fn on_drag(&mut self, message: DragMsg) -> Vec<Effect> {
        match message {
            DragMsg::Moved(at) => {
                if let Some((panel, row)) = self.drag.gesture.moved(at) {
                    self.drag.names = gesture::dragged_names(self.pane(panel), row);
                    if self.drag.names.is_empty() {
                        self.drag.gesture = Gesture::Idle;
                    }
                }
            }
            DragMsg::Released => {
                let Some(from) = self.drag.gesture.release() else {
                    return self.click_released();
                };
                let names = std::mem::take(&mut self.drag.names);
                return self.drop_on_panel(from, names);
            }
            DragMsg::Left => {
                if let Gesture::Dragging { panel } = self.drag.gesture
                    && drag_out::AVAILABLE
                {
                    self.drag.gesture = Gesture::Idle;
                    let names = std::mem::take(&mut self.drag.names);
                    return self.drag_out(panel, names);
                }
            }
            DragMsg::Ended {
                panel,
                outcome,
                fetched,
            } => return self.drag_ended(panel, outcome, fetched),
            DragMsg::FileDropped(path) => {
                let first = self.drag.dropped.is_empty();
                if self.drag.dropped.len() < dropped::MAX_PATHS {
                    self.drag.dropped.push(path);
                }
                if first {
                    let settle = Task::future(tokio::time::sleep(SETTLE))
                        .then(|()| drag_out::pointer())
                        .map(|pointer| wrap(DragMsg::Settled(pointer)));
                    return vec![Effect::Task(settle)];
                }
            }
            DragMsg::Settled(pointer) => {
                let paths = std::mem::take(&mut self.drag.dropped);
                let across = pointer.map(|pointer| pointer.across);
                let share = self.resize.left_share();
                let panel = dropped::target_panel(across, share, self.maximized())
                    .unwrap_or_else(|| self.hovered.unwrap_or(self.active));
                let moving = pointer.map_or(self.modifiers_held().shift, |pointer| pointer.shift);
                return self.copy_in(panel, paths, moving);
            }
            DragMsg::Fetch(yes) => {
                if let Some(fetch) = self.drag.question.take()
                    && yes
                {
                    return self.begin_fetch(fetch);
                }
            }
            DragMsg::Session(result) => return self.session_made(result),
            DragMsg::Copied { count, cut, ok } => self.copied(count, cut, ok),
            DragMsg::Pasted(files) => return self.pasted(files),
            DragMsg::ClickClears(on) => self.set_click_clears(on),
        }
        Vec::new()
    }

    /// The release of a drag from `from`: over the other panel's file list the copy dialog (the
    /// move dialog with Shift) opens for the dragged entries, as F5 and F6 open it.
    fn drop_on_panel(&mut self, from: PanelId, names: Vec<String>) -> Vec<Effect> {
        let Some(to) = self.hovered.filter(|panel| *panel != from) else {
            return Vec::new();
        };
        if names.is_empty() || self.pane(to).state.view != PanelView::Files {
            return Vec::new();
        }
        let kind = if self.modifiers_held().shift {
            FileDialog::Move
        } else {
            FileDialog::Copy
        };
        let effects = self.open_file_dialog(from, kind);
        if let Some(dialog) = self.file_ops.dialog.as_mut() {
            dialog.take_names(names);
        }
        effects
    }

    /// The drag left the window: local entries go out at once, remote ones are fetched first.
    fn drag_out(&mut self, panel: PanelId, names: Vec<String>) -> Vec<Effect> {
        let folder = self.pane(panel).state.path.clone();
        if names.is_empty() {
            return Vec::new();
        }
        if fetch::remote(&folder) {
            return self.fetch(panel, names, Purpose::Drag);
        }
        let paths = names.iter().map(|name| folder.path.join(name)).collect();
        start_drag(panel, paths, None)
    }

    fn drag_ended(
        &mut self,
        panel: PanelId,
        outcome: DragOutcome,
        fetched: Option<PathBuf>,
    ) -> Vec<Effect> {
        match outcome {
            // The target may have moved the files: the panel is read again. Fetched copies the
            // target may have moved away are not offered again.
            DragOutcome::Ended => {
                if let Some(folder) = fetched {
                    self.drag.fetched.retain(|_, kept| *kept != folder);
                }
                return vec![Effect::Dispatch(
                    panel,
                    vec![Command::Refresh { panel: Some(panel) }],
                )];
            }
            // The button came up during the fetch; the copies wait for the next drag.
            DragOutcome::NotHeld if fetched.is_some() => {
                self.info = Some(t!("drag.fetched_hint").to_owned());
            }
            DragOutcome::NotHeld => {}
            DragOutcome::Failed => {
                self.pane_mut(panel).error = Some(t!("drag.failed").to_owned());
            }
        }
        Vec::new()
    }

    /// Files from another program into the folder of `panel`: the queue copies them (moves them
    /// with `moving`), with its progress window and conflict questions.
    fn copy_in(&mut self, panel: PanelId, paths: Vec<PathBuf>, moving: bool) -> Vec<Effect> {
        if self.file_ops.modal() || self.drag.question.is_some() {
            self.pane_mut(panel).error = Some(t!("drop.busy").to_owned());
            return Vec::new();
        }
        if self.pane(panel).state.view != PanelView::Files {
            self.pane_mut(panel).error = Some(t!("drop.files_only").to_owned());
            return Vec::new();
        }
        let (paths, refused) = dropped::accept(paths, cfg!(windows));
        if refused > 0 {
            self.info = Some(t!("drop.refused", count = refused));
        }
        let target = self.pane(panel).state.path.clone();
        let other = self.pane(panel.other()).state.path.clone();
        match dropped::transfer(panel, &target, &other, paths, moving) {
            Ok(command) => vec![Effect::Ops(Submit {
                panel,
                command,
                follow: FollowUp::Refresh,
                background: false,
            })],
            Err(dropped::Refusal::Nothing) => Vec::new(),
            Err(dropped::Refusal::NoLocalPanel) => {
                self.pane_mut(panel).error = Some(t!("drop.needs_local").to_owned());
                Vec::new()
            }
        }
    }
}

/// The drag of `paths` out of the window, from `panel`; nothing where the system has none.
fn start_drag(panel: PanelId, paths: Vec<PathBuf>, fetched: Option<PathBuf>) -> Vec<Effect> {
    drag_out::start(paths)
        .map(|task| {
            Effect::Task(task.map(move |outcome| {
                wrap(DragMsg::Ended {
                    panel,
                    outcome,
                    fetched: fetched.clone(),
                })
            }))
        })
        .into_iter()
        .collect()
}

#[cfg(test)]
#[path = "drag_tests.rs"]
mod tests;
