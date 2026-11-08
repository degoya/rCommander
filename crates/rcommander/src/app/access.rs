//! The app's ways in from outside and from its own modules: the engine (tests), a panel's pane,
//! and [`App::update`], which applies a message and runs what it leaves to do.

use iced::Task;
use rc_engine::PanelId;

#[cfg(test)]
use rc_engine::Engine;

use super::{App, Message, panel::Pane};

impl App {
    #[cfg(test)]
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn pane(&self, panel: PanelId) -> &Pane {
        &self.panes[panel.index()]
    }

    pub(super) fn pane_mut(&mut self, panel: PanelId) -> &mut Pane {
        &mut self.panes[panel.index()]
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let effects = self.apply(message);
        let mut tasks: Vec<Task<Message>> = effects
            .into_iter()
            .map(|effect| self.perform(effect))
            .collect();
        tasks.push(self.request_icons());
        tasks.push(self.request_thumbnails());
        tasks.push(self.sync_dialog_windows());
        Task::batch(tasks)
    }
}
