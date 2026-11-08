//! The bar "Default" of the first start (RC-100-87, owner 2026-11-07): rows of the standard
//! keymap, grouped by separators. Migration 0033 writes it once into the database; this is the
//! same bar as a value, which `default_bar_tests.rs` holds against the migration's JSON and the
//! keymap's rows. A button without a symbol shows its command's name.

use super::{BarButton, BarItem, BarSpec, ButtonAction, SPEC_VERSION};
use crate::user_commands::{CommandKind, CommandRef};

/// The bar's name, as the migration stores it.
pub const DEFAULT_BAR_NAME: &str = "Default";

/// The keymap rows of the bar, `None` a separator.
pub const DEFAULT_ROWS: &[Option<&str>] = &[
    Some("panel.refresh"),
    None,
    Some("view_brief"),
    Some("view_full"),
    None,
    Some("panel.invert_selection"),
    None,
    Some("archive.pack"),
    Some("archive.extract"),
    None,
    Some("files.find_dialog"),
    Some("panel.toggle_quick_view"),
    Some("multi_rename.open_dialog"),
    Some("sync.open_dialog"),
    None,
    Some("panel.toggle_terminal"),
];

/// The bar "Default".
#[must_use]
pub fn default_spec() -> BarSpec {
    let items = DEFAULT_ROWS
        .iter()
        .map(|row| match row {
            Some(row) => BarItem::Button(BarButton {
                action: ButtonAction::Command(CommandRef {
                    kind: CommandKind::Internal,
                    command: (*row).to_owned(),
                    ..CommandRef::default()
                }),
                icon: None,
                tooltip: String::new(),
            }),
            None => BarItem::Separator,
        })
        .collect();
    BarSpec {
        version: SPEC_VERSION,
        items,
    }
}

#[cfg(test)]
#[path = "default_bar_tests.rs"]
mod tests;
