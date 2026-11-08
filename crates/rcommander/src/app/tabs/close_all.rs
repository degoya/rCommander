//! "Close all tabs" from a tab's menu (RC-100-92, Total Commander's `cm_CloseAllTabs`): every tab
//! of the panel but the shown one closes; while `tabs.confirm_close_all` is on a small question
//! offers to keep the locked tabs or to close them too (`Popup::close_all`).

use rc_engine::PanelId;

use super::TabsMessage;
use crate::app::{App, Effect, Message};

impl App {
    /// "Close all tabs": asks first while `tabs.confirm_close_all` is on (RC-100-92).
    pub(in crate::app) fn close_all_tabs(&mut self, panel: PanelId) -> Vec<Effect> {
        let Some(db) = self.db.clone() else {
            return self.on_tabs(TabsMessage::CloseAll(panel, false));
        };
        let read = async move {
            rc_engine::settings::stored_bool(&db, rc_engine::tabs::CONFIRM_CLOSE_ALL_KEY).await
        };
        let task = iced::Task::future(read)
            .map(move |ask| Message::Tabs(TabsMessage::CloseAll(panel, ask)));
        vec![Effect::Task(task)]
    }
}
