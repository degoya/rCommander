//! The icons on the dialogs' buttons (RC-110-39): the setting `appearance.button_icons`, read at
//! the start and after every change, switches them on or off for every dialog at once
//! (`theme::layout::set_icons`).

use iced::Task;
use rc_db::Db;
use rc_engine::{
    EngineEvent,
    settings::{BUTTON_ICONS_KEY, stored_bool},
};

use crate::app::{App, Effect, Message};

impl App {
    /// A change of the setting is read here.
    pub(in crate::app) fn button_icons_intercept(
        &mut self,
        message: &Message,
    ) -> Option<Vec<Effect>> {
        match message {
            Message::Engine(EngineEvent::SettingChanged { key }) if key == BUTTON_ICONS_KEY => {
                Some(vec![Effect::Task(self.button_icons_task())])
            }
            _ => None,
        }
    }

    /// Reads [`icons_shown`].
    pub(in crate::app) fn button_icons_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(icons_shown(db), Message::ButtonIcons)
    }
}

/// Whether the dialogs' buttons show their icons; on unless switched off.
async fn icons_shown(db: Db) -> bool {
    stored_bool(&db, BUTTON_ICONS_KEY).await
}

#[cfg(test)]
#[path = "button_icons_tests.rs"]
mod tests;
