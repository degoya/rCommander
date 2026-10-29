//! F10 and quitting (owner, 2026-10-18; RC-080-17).
//!
//! With a dialog open, F10 closes it exactly as Escape does: the key becomes Escape before any
//! dialog sees it ([`App::f10_key`]), also when a text field captured it, so every rule of
//! Escape (`escape.rs`) holds for F10 too. Without a dialog, F10 is the keymap's `app.quit` in
//! the file list; in a terminal it stays the shell's, as every F key. The main window's X and
//! Alt+F4 run `app.quit` too (`dialog_windows.rs`), so they ask the same. The engine announces the
//! wish ([`EngineEvent::Quit`]); with jobs running or waiting the window asks first - Quit or
//! Cancel, Cancel the default, Enter and Escape cancel - otherwise it stores the tabs and the
//! places and ends ([`App::save_and_exit`]). The setting `keys.f10` narrows the key to closing
//! dialogs or switches it off; it is read at the start and whenever it changes. The command
//! palette's row runs `app.quit` whatever the setting says.
//!
//! A lost graphics device (`gpu_loss.rs`) asks in the system's message box whether to restart,
//! unless `appearance.gpu_loss_notice` is off; yes leaves as a confirmed quit does - the queue's
//! jobs are kept - and the program starts again once the window has closed.

use iced::{
    Element, Subscription, Task,
    futures::{FutureExt as _, Stream},
    widget::column,
};
use rc_engine::{
    Chord, EngineEvent,
    settings::{F10_KEY, GPU_LOSS_NOTICE_KEY, stored_bool, stored_setting},
};

use super::{
    App, Effect, Message,
    escape::{enter, escape},
    field_walk,
};
use crate::{
    gpu_loss,
    i18n::t,
    platform,
    theme::{self, Tokens, fit::label, layout},
};

/// What F10 does, as the setting `keys.f10` says.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum F10Mode {
    /// Closes the open dialog; without one it quits.
    #[default]
    CloseAndQuit,
    /// Closes the open dialog; without one it does nothing.
    CloseDialogs,
    /// Nothing.
    Off,
}

impl F10Mode {
    /// The mode a stored value names; the default for anything else.
    pub fn from_value(value: Option<&str>) -> Self {
        match value {
            Some("close_dialogs") => Self::CloseDialogs,
            Some("off") => Self::Off,
            _ => Self::CloseAndQuit,
        }
    }
}

/// F10's mode, the question before quitting and the way out.
#[derive(Debug, Default)]
pub struct Quit {
    pub f10: F10Mode,
    /// The question while it is open: how many jobs run or wait.
    pub asking: Option<usize>,
    /// The tabs and places are being stored on the way out; a second quit starts nothing.
    pub leaving: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitMsg {
    /// The setting `keys.f10`, read.
    Mode(F10Mode),
    /// Quit in the question.
    Confirm,
    /// Cancel in the question, Enter or Escape.
    Cancel,
    /// The graphics device is lost (`gpu_loss.rs`).
    GpuLost,
    /// The answer to the restart after a lost device.
    Restart(bool),
}

/// The F10 key without modifiers.
pub fn f10() -> Chord {
    Chord::plain("F10").expect("F10 is a named key")
}

impl App {
    /// `message`, with F10 as the setting wants it: Escape while a dialog is open; nothing when
    /// it would quit and the setting says otherwise, or when the setting switched it off.
    pub(super) fn f10_key(&self, message: Message) -> Option<Message> {
        let (Message::Key(chord) | Message::KeyCaptured(chord)) = &message else {
            return Some(message);
        };
        if *chord != f10() {
            return Some(message);
        }
        match self.quit.f10 {
            F10Mode::Off => None,
            _ if self.dialog_open() => Some(Message::Key(escape())),
            F10Mode::CloseDialogs => None,
            F10Mode::CloseAndQuit => Some(message),
        }
    }

    /// The quit's share of every message: the engine's wish to quit, a changed `keys.f10`, the
    /// question's buttons, and while it is open every key and click. `None` for a message it
    /// leaves to the rest of the window.
    pub(super) fn quit_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        match message {
            Message::Engine(EngineEvent::Quit { unfinished }) => {
                Some(self.quit_wanted(*unfinished))
            }
            Message::Engine(EngineEvent::SettingChanged { key }) if key == F10_KEY => {
                Some(vec![Effect::Task(self.f10_task())])
            }
            Message::Quit(message) => Some(self.on_quit(*message)),
            _ if self.quit.asking.is_none() => None,
            // Tab and Shift+Tab walk the two buttons (RC-080-16).
            Message::Key(chord) if field_walk::tab(chord).is_some() => field_walk::walk_key(chord),
            // Enter on no focused button takes the default, Cancel; a focused one took it.
            Message::Key(chord) if *chord == escape() || *chord == enter() => {
                Some(self.on_quit(QuitMsg::Cancel))
            }
            Message::Key(_) | Message::KeyCaptured(_) | Message::Press => Some(Vec::new()),
            _ => None,
        }
    }

    /// `app.quit` announced: the question when jobs run or wait, else the way out.
    fn quit_wanted(&mut self, unfinished: usize) -> Vec<Effect> {
        if self.quit.leaving {
            return Vec::new();
        }
        if unfinished > 0 {
            self.quit.asking = Some(unfinished);
            return Vec::new();
        }
        self.leave()
    }

    pub(super) fn on_quit(&mut self, message: QuitMsg) -> Vec<Effect> {
        match message {
            QuitMsg::Mode(mode) => self.quit.f10 = mode,
            // The question closes at once, so a second click cannot send it again.
            QuitMsg::Confirm if self.quit.asking.take().is_some() => return self.leave(),
            QuitMsg::Confirm | QuitMsg::Cancel => self.quit.asking = None,
            QuitMsg::GpuLost => return vec![Effect::Task(self.gpu_loss_task())],
            QuitMsg::Restart(true) if !self.quit.leaving => {
                gpu_loss::want_restart();
                return self.leave();
            }
            QuitMsg::Restart(_) => {}
        }
        Vec::new()
    }

    /// Asks in the system's message box whether to restart, unless the setting says not to.
    fn gpu_loss_task(&self) -> Task<Message> {
        let db = self.db.clone();
        let title = t!("gpu_loss.title").to_owned();
        let text = t!("gpu_loss.text").to_owned();
        Task::perform(
            async move {
                let notice = match db {
                    Some(db) => stored_bool(&db, GPU_LOSS_NOTICE_KEY).await,
                    None => true,
                };
                // The box waits for its answer on a thread of its own.
                notice
                    && tokio::task::spawn_blocking(move || {
                        platform::gpu_loss::ask_restart(&title, &text)
                    })
                    .await
                    .unwrap_or(false)
            },
            |restart| Message::Quit(QuitMsg::Restart(restart)),
        )
    }

    /// Stores the tabs, the places and the windows' geometry, then ends the app. The queue's
    /// unfinished jobs are kept by the engine for the next start. With "install when quitting"
    /// a downloaded update is handed to the updater first (RC-100-07); the engine decides.
    fn leave(&mut self) -> Vec<Effect> {
        self.quit.leaving = true;
        let engine = self.engine.clone();
        let mut exit = Some(self.save_and_exit());
        let handed = Task::perform(async move { engine.update_on_quit().await }, |_| ());
        vec![Effect::Task(
            handed.then(move |()| exit.take().unwrap_or_else(Task::none)),
        )]
    }

    /// Reads `keys.f10` (at the start and when it changed).
    pub(super) fn f10_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(f10_mode(db), |mode| Message::Quit(QuitMsg::Mode(mode)))
    }

    /// The question over the window while it is open.
    pub(super) fn quit_layer(&self, tokens: &'static Tokens) -> Option<Element<'_, Message>> {
        let title = label(t!("quit.title")).font(theme::UI_FONT_STRONG);
        Some(layout::dialog_card(
            Message::dialog_head(title),
            self.quit_body()?,
            layout::SMALL_DIALOG_WIDTH,
            tokens,
        ))
    }

    /// The question's text and buttons while it is open: on its card, or filling a window of
    /// its own (RC-080-23).
    pub(super) fn quit_body(&self) -> Option<Element<'_, Message>> {
        let unfinished = self.quit.asking?;
        let buttons = layout::button_row(
            Some(layout::primary_button(
                t!("quit.cancel"),
                Some(Message::Quit(QuitMsg::Cancel)),
            )),
            [layout::secondary_button(
                t!("quit.confirm"),
                Some(Message::Quit(QuitMsg::Confirm)),
            )],
            [],
        );
        Some(
            column![label(t!("quit.text", count = unfinished)), buttons]
                .spacing(10)
                .into(),
        )
    }
}

/// The lost graphics device, once (`gpu_loss.rs`).
pub fn gpu_lost() -> Subscription<Message> {
    Subscription::run(lost)
}

fn lost() -> impl Stream<Item = Message> {
    gpu_loss::lost()
        .map(|()| Message::Quit(QuitMsg::GpuLost))
        .into_stream()
}

/// The stored `keys.f10`, or its default.
pub async fn f10_mode(db: rc_db::Db) -> F10Mode {
    let value = stored_setting(&db, F10_KEY).await;
    F10Mode::from_value(value.as_ref().and_then(|value| value.as_str()))
}

#[cfg(test)]
#[path = "quit_tests.rs"]
mod tests;
