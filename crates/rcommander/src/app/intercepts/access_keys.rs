//! The access keys' share of the window's messages (RC-080-16): the underline follows Alt as the
//! modifiers change - the file operations keep their own copy of them - and the setting
//! `keys.underline_access_keys`, read at the start and after every change, keeps it always on.
//! The keys themselves are pressed in the widget tree, by the scope round each window's content
//! ([`App::window_root`], `theme::mnemonic`).

use iced::{Element, Task, window};
use rc_db::Db;
use rc_engine::{
    EngineEvent,
    settings::{UNDERLINE_ACCESS_KEYS_KEY, stored_bool},
};

use crate::{
    app::{App, Effect, Message, file_ops::FileOpsMsg},
    theme::mnemonic,
};

impl App {
    /// What window `id` shows ([`App::window_view`]), its dialogs answering to their access keys
    /// - except while a shortcut is recorded, when every key is the recording's.
    pub(in crate::app) fn window_root(&self, id: window::Id) -> Element<'_, Message> {
        mnemonic::Scope::new(self.window_view(id))
            .paused(self.records_shortcut())
            .into()
    }

    /// A change of the modifiers passes on to the file operations; a change of the setting is
    /// read here.
    pub(in crate::app) fn access_keys_intercept(
        &mut self,
        message: &Message,
    ) -> Option<Vec<Effect>> {
        match message {
            Message::FileOps(FileOpsMsg::Modifiers(modifiers)) => {
                mnemonic::set_modifiers(*modifiers);
                None
            }
            Message::Engine(EngineEvent::SettingChanged { key })
                if key == UNDERLINE_ACCESS_KEYS_KEY =>
            {
                Some(vec![Effect::Task(self.access_keys_task())])
            }
            _ => None,
        }
    }

    /// Reads [`underline_always`].
    pub(in crate::app) fn access_keys_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(underline_always(db), Message::AccessKeys)
    }
}

/// Whether the access keys are always underlined; off unless switched on.
async fn underline_always(db: Db) -> bool {
    stored_bool(&db, UNDERLINE_ACCESS_KEYS_KEY).await
}

#[cfg(test)]
#[path = "access_keys_tests.rs"]
mod tests;
