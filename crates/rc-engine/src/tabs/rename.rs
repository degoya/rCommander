//! A tab's own name and closing all tabs at once (RC-100-92): Total Commander's tab menu "Rename
//! tab..." (`cm_RenameTab`) and `cm_CloseAllTabs`, which "closes all tabs except for locked tabs.
//! The current dir will be kept."
//!
//! A tab a person named keeps the name in every folder it shows, until it is cleared; the name is
//! stored with the tabs and carried by a workspace. Closing all tabs keeps the active one, and the
//! locked ones unless asked to close them too; the window asks first while
//! `tabs.confirm_close_all` is on.

use crate::{Engine, EngineError, EngineEvent, Outcome, PanelChange, PanelId, TabsError};

use super::TabId;

/// The longest name a tab takes, in characters.
pub const MAX_TAB_TITLE: usize = 64;

/// Whether the window asks before it closes all tabs.
pub const CONFIRM_CLOSE_ALL_KEY: &str = "tabs.confirm_close_all";

/// A tab's name as it is kept: trimmed, `None` for a blank one (the folder's name again).
///
/// # Errors
///
/// [`TabsError::TabTitleInvalid`] for a name longer than [`MAX_TAB_TITLE`] characters or one
/// holding a control character.
pub fn checked_title(title: &str) -> Result<Option<String>, TabsError> {
    let title = title.trim();
    if title.is_empty() {
        return Ok(None);
    }
    if title.chars().count() > MAX_TAB_TITLE {
        return Err(TabsError::TabTitleInvalid {
            reason: "longer than 64 characters",
        });
    }
    if title.chars().any(char::is_control) {
        return Err(TabsError::TabTitleInvalid {
            reason: "control characters",
        });
    }
    Ok(Some(title.to_owned()))
}

impl Engine {
    /// `tabs.rename`: tab `tab` (the active one by default) of `panel` takes `title`; blank or
    /// `None`, it shows its folder's name again.
    pub(crate) fn rename_tab(
        &self,
        panel: Option<PanelId>,
        tab: Option<TabId>,
        title: Option<&str>,
    ) -> Result<Outcome, EngineError> {
        let title = checked_title(title.unwrap_or_default())?;
        let id = self.resolve(panel);
        {
            let mut state = self.state();
            let tab = match tab {
                Some(tab) => tab,
                None => state.tabs.set(id).active_tab().id,
            };
            if !state.tabs.rename(id, tab, title) {
                return Err(TabsError::TabNotFound { tab }.into());
            }
        }
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Tabs,
        });
        Ok(self.tabs_outcome(id))
    }

    /// `tabs.close_all`: every tab of the panel but the active one, the locked ones only with
    /// `include_locked`. The panel keeps showing what it shows.
    pub(crate) fn close_all_tabs(&self, panel: Option<PanelId>, include_locked: bool) -> Outcome {
        let id = self.resolve(panel);
        let closed = self.state().tabs.close_all(id, include_locked);
        if closed > 0 {
            self.publish(EngineEvent::Panel {
                panel: id,
                change: PanelChange::Tabs,
            });
        }
        self.tabs_outcome(id)
    }
}

#[cfg(test)]
#[path = "rename_tests.rs"]
mod tests;
