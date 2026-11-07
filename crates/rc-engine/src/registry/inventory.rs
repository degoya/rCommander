//! Server inventories (RC-100-39): the sources, a sync of one, and setting one up.

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "inventory.refresh",
        class: CommandClass::Write,
        summary: "Sync an inventory source's folder of connections with the hosts its inventory lists now; with preview only the plan",
    },
    CommandSpec {
        id: "inventory.source_delete",
        class: CommandClass::GuiOnly,
        summary: "Remove an inventory source and its stored token; its connections stay",
    },
    CommandSpec {
        id: "inventory.source_save",
        class: CommandClass::GuiOnly,
        summary: "Set up or change an inventory source: plugin, address, folder and the token it sends",
    },
    CommandSpec {
        id: "inventory.sources",
        class: CommandClass::Read,
        summary: "The inventory sources with their last sync, and the installed inventory plugins; never a token",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "inventory.refresh" => RefreshInventory {
                source: InventorySourceId,
                /// Only the plan: what would be created, updated and marked.
                #[serde(default)]
                preview: bool,
            },
            "inventory.source_delete" => DeleteInventorySource { source: InventorySourceId },
            "inventory.source_save" => SaveInventorySource(InventorySourceRequest),
            "inventory.sources" => ListInventorySources {},
        } }
    };
}
pub(super) use commands;

#[cfg(test)]
#[path = "inventory_tests.rs"]
mod tests;
