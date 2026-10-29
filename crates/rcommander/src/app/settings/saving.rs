//! Apply and OK: what changed is stored off the window's thread, and the answer comes back to
//! the dialog - or, once Escape closed the dialog while it was saving, to the window: the MCP
//! server's new state always, a failure in the status bar (RC-080-44). Split from `settings.rs`
//! to keep it under 500 lines.

use iced::Task;

use super::{Changes, SettingsMessage, save, wrap};
use crate::{
    app::{App, Effect},
    i18n::{error_text, t},
};

impl App {
    pub(super) fn save_settings(&mut self, close: bool) -> Vec<Effect> {
        let (Some(dialog), Some(db)) = (self.settings.as_mut(), self.db.clone()) else {
            return Vec::new();
        };
        let Some(draft) = &dialog.draft else {
            return Vec::new();
        };
        let changes = match draft.changes() {
            Ok(changes) => changes,
            Err(invalid) => {
                let field = crate::i18n::lookup(invalid.label);
                dialog.note = Some((t!("settings.invalid", field = field), true));
                return Vec::new();
            }
        };
        if changes.is_empty() {
            if close {
                self.settings = None;
            }
            return Vec::new();
        }
        dialog.busy = true;
        dialog.saving = true;
        let (engine, server) = (self.engine.clone(), self.mcp_server.clone());
        let stored = Box::new(changes.clone());
        let task = Task::perform(save::store(engine, db, server, changes), move |result| {
            wrap(SettingsMessage::Saved {
                result,
                changes: stored,
                close,
            })
        });
        vec![Effect::Task(task)]
    }

    pub(super) fn settings_saved(
        &mut self,
        result: save::Stored,
        changes: &Changes,
        close: bool,
    ) -> Vec<Effect> {
        if let Some(status) = result.mcp {
            self.mcp = status;
        }
        // The dialog that saved; not one opened again since Escape closed it.
        let Some(dialog) = self.settings.as_mut().filter(|dialog| dialog.saving) else {
            if let Some(code) = result.error {
                self.info = Some(t!("settings.not_saved", error = error_text(code)));
            }
            return Vec::new();
        };
        dialog.saving = false;
        match result.error {
            None => {
                if let Some(draft) = &mut dialog.draft {
                    draft.stored(changes);
                }
                if close {
                    self.settings = None;
                } else {
                    dialog.busy = false;
                    dialog.note = Some((t!("settings.saved").to_owned(), false));
                }
            }
            Some(code) => dialog.fail(code),
        }
        Vec::new()
    }
}
