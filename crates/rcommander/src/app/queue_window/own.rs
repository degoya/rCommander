//! The queue in a native window of its own (owner, 2026-10-18; RC-010-14, RC-080-15): the owner
//! wants to move it and work on while the queue runs - with `queue.window_own` on,
//! the queue window opens beside the main window (`dialog_windows.rs`, kind `Queue`), owned by it
//! but modeless: the main window stays enabled and its keys go to the panels. The queue window
//! takes the keys only while it has the keyboard, which the system's focus events tell
//! ([`QueueMsg::Focused`]); Escape, F10 and its X close it, the queue runs on. Ctrl+Shift+Q brings
//! it to the front, "Answer" brings the main window, where the question is, to the front.

use iced::{Task, window};
use rc_engine::{
    EngineEvent,
    settings::{QUEUE_WINDOW_OWN_KEY, stored_bool},
};

use super::QueueMsg;
use crate::app::{App, Effect, Message, dialog_windows::DialogWindow, field_walk};

impl App {
    /// The queue's share of every message before any dialog's: the focus moving between the
    /// windows, a changed `queue.window_own`, and the keys while its own window has the keyboard.
    pub(in crate::app) fn queue_window_first(&mut self, message: &Message) -> Option<Vec<Effect>> {
        match message {
            Message::Queue(QueueMsg::Focused(id, focused)) => {
                self.dialog_windows.focus(*id, *focused);
                Some(Vec::new())
            }
            Message::Engine(EngineEvent::SettingChanged { key }) if key == QUEUE_WINDOW_OWN_KEY => {
                Some(vec![Effect::Task(self.queue_own_task())])
            }
            Message::Key(_) | Message::KeyCaptured(_)
                if self.queue_window.is_some()
                    && self.dialog_windows.has_focus(DialogWindow::Queue) =>
            {
                Some(self.own_window_key(message))
            }
            _ => None,
        }
    }

    /// A key in the queue's own window: Tab walks its controls, a key a control took stays its
    /// own, every other key is the window's (`queue_key`).
    fn own_window_key(&mut self, message: &Message) -> Vec<Effect> {
        match message {
            Message::Key(chord) if field_walk::tab(chord).is_some() => {
                field_walk::walk_key(chord).unwrap_or_default()
            }
            Message::KeyCaptured(chord) if field_walk::taken(chord) => Vec::new(),
            Message::Key(chord) | Message::KeyCaptured(chord) => self.queue_key(chord),
            _ => Vec::new(),
        }
    }

    /// Whether the queue window has the keys: lying over the main window, or in its own window
    /// while that has the keyboard. Escape and F10 close it then (`escape.rs`, `quit.rs`).
    pub(in crate::app) fn queue_has_keys(&self) -> bool {
        self.queue_window.is_some()
            && (!self.dialog_windows.queue_own
                || self.dialog_windows.has_focus(DialogWindow::Queue))
    }

    /// Reads `queue.window_own`; at the start and when it changes.
    pub(in crate::app) fn queue_own_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(
            async move { stored_bool(&db, QUEUE_WINDOW_OWN_KEY).await },
            |own| Message::Queue(QueueMsg::Own(own)),
        )
    }

    /// The queue's own window to the front, while it is open.
    pub(super) fn queue_window_front(&self) -> Vec<Effect> {
        self.dialog_windows
            .id_of(DialogWindow::Queue)
            .map(|id| Effect::Task(window::gain_focus(id)))
            .into_iter()
            .collect()
    }

    /// The main window to the front, while the queue has a window of its own.
    pub(super) fn main_window_front(&self) -> Vec<Effect> {
        let own = self.dialog_windows.id_of(DialogWindow::Queue).is_some();
        self.dialog_windows
            .main()
            .filter(|_| own)
            .map(|id| Effect::Task(window::gain_focus(id)))
            .into_iter()
            .collect()
    }
}

#[cfg(test)]
#[path = "own_tests.rs"]
mod tests;
