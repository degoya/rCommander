//! Folder tabs, the folder history and the moves between panels (RC-010-03), after Total
//! Commander: each panel has a row of tabs (Ctrl+T, Ctrl+W, Ctrl+Tab), a tab can be locked so a
//! new folder opens in a new tab, each tab keeps its folder history (Alt+Left, Alt+Right,
//! Alt+Down), and the panels swap (Ctrl+U) or take each other's folder (Ctrl+Left, Ctrl+Right).
//!
//! The panel's [`crate::PanelState`] is always the active tab's state, so every other command
//! works on the active tab unchanged. `panel.navigate` and everything else that shows a folder
//! goes through `show_path`, which records the visit in the tab's history and, for a locked tab,
//! opens the folder in a new tab instead. The tabs are stored when the window closes and come
//! back at the next start ([`crate::Engine::save_tabs`], [`crate::Engine::restore_tabs`]). The
//! hotlist is `crate::hotlist`.

mod commands;
mod error;
pub(crate) mod history;
mod imported;
mod moves;
mod persist;
pub(crate) mod requests;
mod set;
pub(crate) mod title;

pub use error::TabsError;
pub use history::HISTORY_LIMIT;
pub use persist::Relocated;
pub(crate) use set::{Tab, TabSet, Tabs};
pub use set::{TabId, TabView};

/// One command of each tab, history and hotlist variant, for the registry's tests.
#[cfg(test)]
pub(crate) fn one_of_each() -> Vec<crate::Command> {
    use crate::{Command, PanelId};
    use requests::{
        AddHotlistFolderRequest, AddHotlistRequest, HistoryJumpRequest, LockTabRequest,
        MoveHotlistRequest, MoveTabRequest, NextTabRequest, OpenHotlistRequest, OpenTabRequest,
        PanelRequest, SelectTabRequest, SwapRequest, TabRequest, TransferDirRequest,
        UpdateHotlistRequest,
    };
    let left = PanelRequest {
        panel: Some(PanelId::Left),
    };
    vec![
        Command::ListTabs(left.clone()),
        Command::OpenTab(OpenTabRequest {
            cursor: true,
            other: true,
            ..OpenTabRequest::default()
        }),
        Command::CloseTab(TabRequest {
            panel: None,
            tab: Some(TabId(2)),
        }),
        Command::SelectTab(SelectTabRequest {
            panel: None,
            tab: TabId(1),
        }),
        Command::NextTab(NextTabRequest {
            panel: None,
            backwards: true,
        }),
        Command::LockTab(LockTabRequest {
            locked: Some(true),
            ..LockTabRequest::default()
        }),
        Command::CopyTabToOther(TabRequest::default()),
        Command::MoveTab(MoveTabRequest {
            panel: Some(PanelId::Right),
            tab: TabId(3),
            to: 0,
        }),
        Command::SwapPanels(SwapRequest { with_tabs: true }),
        Command::TransferDir(TransferDirRequest {
            panel: None,
            side: PanelId::Right,
            path: None,
        }),
        Command::HistoryBack(left.clone()),
        Command::HistoryForward(left.clone()),
        Command::HistoryList(left),
        Command::HistoryJump(HistoryJumpRequest {
            panel: None,
            index: 3,
        }),
        Command::ListHotlist {},
        Command::AddHotlist(AddHotlistRequest {
            name: Some("Projects".into()),
            ..AddHotlistRequest::default()
        }),
        Command::AddHotlistFolder(AddHotlistFolderRequest {
            name: "Work".into(),
            parent_id: None,
        }),
        Command::UpdateHotlist(UpdateHotlistRequest {
            id: 1,
            panel: None,
            name: None,
            path: Some("..".into()),
            target: None,
            clear_target: true,
        }),
        Command::MoveHotlist(MoveHotlistRequest {
            id: 1,
            parent_id: Some(2),
            position: Some(0),
        }),
        Command::DeleteHotlist { id: 1 },
        Command::OpenHotlist(OpenHotlistRequest { id: 1, panel: None }),
    ]
}
