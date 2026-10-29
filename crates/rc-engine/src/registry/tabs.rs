//! Folder tabs and the moves between the two panels (RC-010-03).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "panel.swap",
        class: CommandClass::Read,
        summary: "Swap the folders of the two panels (Ctrl+U), or their whole tab rows (Ctrl+Shift+U)",
    },
    CommandSpec {
        id: "panel.transfer",
        class: CommandClass::Read,
        summary: "Show the folder under a panel's cursor, or the panel's own, in the left or right panel (Ctrl+Left, Ctrl+Right)",
    },
    CommandSpec {
        id: "tabs.close",
        class: CommandClass::Read,
        summary: "Close a tab of a panel, the active one by default; the last tab stays",
    },
    CommandSpec {
        id: "tabs.copy_to_other",
        class: CommandClass::Read,
        summary: "Copy a tab of a panel to the end of the other panel's tabs",
    },
    CommandSpec {
        id: "tabs.list",
        class: CommandClass::Read,
        summary: "A panel's tabs: id, title, folder, whether locked and which one is shown",
    },
    CommandSpec {
        id: "tabs.lock",
        class: CommandClass::Read,
        summary: "Lock or unlock a tab; a locked tab opens every new folder in a new tab",
    },
    CommandSpec {
        id: "tabs.move",
        class: CommandClass::Read,
        summary: "Move a tab of a panel to another place in its row, as dragging it in the tab bar does",
    },
    CommandSpec {
        id: "tabs.next",
        class: CommandClass::Read,
        summary: "Show the next tab of a panel, or the previous one (Ctrl+Tab, Ctrl+Shift+Tab)",
    },
    CommandSpec {
        id: "tabs.open",
        class: CommandClass::Read,
        summary: "Open a folder in a new tab: the panel's own, the one under the cursor, or a path; in this or the other panel",
    },
    CommandSpec {
        id: "tabs.select",
        class: CommandClass::Read,
        summary: "Show a tab of a panel by its id",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "tabs.list" => ListTabs(PanelRequest),
            "tabs.open" => OpenTab(OpenTabRequest),
            "tabs.close" => CloseTab(TabRequest),
            "tabs.select" => SelectTab(SelectTabRequest),
            "tabs.next" => NextTab(NextTabRequest),
            "tabs.lock" => LockTab(LockTabRequest),
            "tabs.copy_to_other" => CopyTabToOther(TabRequest),
            "tabs.move" => MoveTab(MoveTabRequest),
            "panel.swap" => SwapPanels(SwapRequest),
            "panel.transfer" => TransferDir(TransferDirRequest),
        } }
    };
}
pub(super) use commands;
