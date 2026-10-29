//! The folder history of a panel's active tab (RC-010-03).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "history.back",
        class: CommandClass::Read,
        summary: "Show the folder before the current one in the tab's history (Alt+Left)",
    },
    CommandSpec {
        id: "history.forward",
        class: CommandClass::Read,
        summary: "Show the folder after the current one in the tab's history (Alt+Right)",
    },
    CommandSpec {
        id: "history.jump",
        class: CommandClass::Read,
        summary: "Show a folder of the tab's history by its index in history.list",
    },
    CommandSpec {
        id: "history.list",
        class: CommandClass::Read,
        summary: "The folders of the tab's history, oldest first, and the one shown (Alt+Down)",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "history.back" => HistoryBack(PanelRequest),
            "history.forward" => HistoryForward(PanelRequest),
            "history.list" => HistoryList(PanelRequest),
            "history.jump" => HistoryJump(HistoryJumpRequest),
        } }
    };
}
pub(super) use commands;
