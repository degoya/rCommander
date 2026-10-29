//! `panel.context_menu`: Shift+F10 and the context-menu key ask the window for the menu of a
//! panel's selection (RC-080-02). The menu is the window's - the Explorer's on a local folder
//! under Windows, rCommander's own elsewhere - so the engine only announces that it is wanted,
//! as `panel.drive_menu` does.

use crate::{Engine, EngineEvent, Outcome, PanelId};

impl Engine {
    /// `panel.context_menu`: announces that the panel's context menu is wanted.
    pub(crate) fn context_menu(&self, panel: Option<PanelId>) -> Outcome {
        let snapshot = self.snapshot(panel);
        self.publish(EngineEvent::ContextMenu {
            panel: snapshot.panel,
        });
        Outcome::Panel(snapshot)
    }
}

#[cfg(test)]
#[path = "context_menu_tests.rs"]
mod tests;
