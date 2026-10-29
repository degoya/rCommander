//! Ctrl+Alt+V in a terminal (RC-040-08): the engine announces the paste, the window reads the
//! setting `terminal.paste_image` and the clipboard's image (`platform::clipboard_image`), the
//! engine puts the PNG where the terminal's program reads it (`Engine::place_terminal_image`)
//! and the path is pasted into the terminal as typed text. What goes wrong is said in the status
//! line.

use iced::Task;
use rc_db::Db;
use rc_engine::{
    TermId,
    settings::{PASTE_IMAGE_KEY, stored_bool},
};

use super::{App, Effect, Message, terminal_pane::TermMessage};
use crate::{
    i18n::{error_text, t},
    platform::clipboard_image,
};

/// The steps of one paste.
#[derive(Debug, Clone)]
pub enum ImageMsg {
    /// Whether the setting allows it.
    Allowed(TermId, bool),
    /// The clipboard's image as a PNG, if it held one.
    Read(TermId, Option<Vec<u8>>),
    /// The path to paste, or the code of what failed.
    Placed(TermId, Result<String, &'static str>),
}

fn wrap(message: ImageMsg) -> Message {
    Message::Terminal(TermMessage::Image(message))
}

/// Whether the setting allows pasting images; on unless switched off.
async fn allowed(db: Option<Db>) -> bool {
    match db {
        Some(db) => stored_bool(&db, PASTE_IMAGE_KEY).await,
        None => true,
    }
}

impl App {
    /// The engine announced a paste for `terminal`.
    pub(super) fn paste_image(&self, terminal: TermId) -> Vec<Effect> {
        let db = self.db.clone();
        let task = Task::perform(allowed(db), move |on| wrap(ImageMsg::Allowed(terminal, on)));
        vec![Effect::Task(task)]
    }

    pub(super) fn on_image(&mut self, message: ImageMsg) -> Vec<Effect> {
        match message {
            ImageMsg::Allowed(_, false) => {
                self.info = Some(t!("terminal.paste_image.off").to_owned())
            }
            ImageMsg::Allowed(terminal, true) => match clipboard_image::read_png() {
                Some(read) => {
                    let task = read.map(move |png| wrap(ImageMsg::Read(terminal, png)));
                    return vec![Effect::Task(task)];
                }
                None => self.info = Some(t!("terminal.paste_image.unsupported").to_owned()),
            },
            ImageMsg::Read(_, None) => {
                self.info = Some(t!("terminal.paste_image.no_image").to_owned())
            }
            ImageMsg::Read(terminal, Some(png)) => {
                let engine = self.engine.clone();
                let place = async move {
                    engine
                        .place_terminal_image(terminal, png)
                        .await
                        .map_err(|error| error.code())
                };
                let task = Task::perform(place, move |placed| {
                    wrap(ImageMsg::Placed(terminal, placed))
                });
                return vec![Effect::Task(task)];
            }
            ImageMsg::Placed(terminal, Ok(path)) => {
                if let Some(widget) = self.terms.widgets.get(&terminal) {
                    widget.paste(&path);
                }
            }
            ImageMsg::Placed(_, Err(code)) => {
                self.info = Some(t!("terminal.paste_image.failed", reason = error_text(code)));
            }
        }
        Vec::new()
    }
}

#[cfg(test)]
#[path = "terminal_image_tests.rs"]
mod tests;
