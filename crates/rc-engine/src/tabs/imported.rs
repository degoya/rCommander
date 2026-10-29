//! Tabs an import brings (RC-050-09): added behind a panel's tabs, or taking their place.

use rc_vfs::VfsPath;

use super::{TabSet, commands::split};
use crate::{Engine, EngineError, EngineEvent, PanelChange, PanelId};

impl Engine {
    /// Panel `panel` gets `tabs` (each a folder and whether it is locked) with the panel's sort
    /// and mode: after its own tabs, the active one staying active; or with `replace` in their
    /// place, the one at `active` shown. Nothing happens for no tabs.
    ///
    /// # Errors
    ///
    /// With `replace`, the folder to show cannot be listed; the tabs stay as they were.
    pub(crate) async fn add_imported_tabs(
        &self,
        panel: PanelId,
        tabs: Vec<(VfsPath, bool)>,
        active: usize,
        replace: bool,
    ) -> Result<(), EngineError> {
        if tabs.is_empty() {
            return Ok(());
        }
        if replace {
            let set = {
                let mut guard = self.state();
                let state = &mut *guard;
                let (live, all) = split(state, panel);
                let made = tabs
                    .into_iter()
                    .map(|(path, locked)| {
                        let mut tab = all.new_tab(path, live.sort, live.mode);
                        tab.locked = locked;
                        tab
                    })
                    .collect();
                TabSet::from_tabs(made, active)
            };
            if let Some(set) = set {
                self.show_tab_set(panel, set).await?;
            }
            return Ok(());
        }
        {
            let mut guard = self.state();
            let state = &mut *guard;
            let (live, all) = split(state, panel);
            all.sync(panel, live);
            for (path, locked) in tabs {
                let mut tab = all.new_tab(path, live.sort, live.mode);
                tab.locked = locked;
                all.push(panel, tab);
            }
        }
        self.publish(EngineEvent::Panel {
            panel,
            change: PanelChange::Tabs,
        });
        Ok(())
    }
}
