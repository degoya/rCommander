//! The bar "Default" of migration 0033 is [`default_spec`], and each of its rows a row of the
//! standard keymap.

use rc_db::Db;

use super::{DEFAULT_BAR_NAME, DEFAULT_ROWS, default_spec};
use crate::{
    Keymap,
    button_bars::{BarItem, BarSpec, ButtonAction},
    user_commands::CommandKind,
};

#[tokio::test]
async fn the_migration_writes_the_default_spec() {
    let db = Db::open_in_memory().await.expect("open");
    let bars = db.button_bars().await.expect("read");
    let first = bars.first().expect("the migration made one");
    assert_eq!(first.name, DEFAULT_BAR_NAME);
    let spec = BarSpec::from_json(&first.spec).expect("reads");
    assert_eq!(spec, default_spec());
}

#[test]
fn every_button_runs_a_row_of_the_standard_keymap() {
    let keymap = Keymap::standard();
    let spec = default_spec();
    assert_eq!(spec.items.len(), DEFAULT_ROWS.len());
    assert_eq!(spec.checked().expect("fits"), spec);
    for item in &spec.items {
        let BarItem::Button(button) = item else {
            continue;
        };
        let ButtonAction::Command(action) = &button.action else {
            panic!("no sub-bar on the default bar");
        };
        assert_eq!(action.kind, CommandKind::Internal);
        assert!(
            keymap
                .bindings()
                .iter()
                .any(|binding| binding.name == action.command),
            "{} is a row of the keymap",
            action.command
        );
        assert!(button.icon.is_none() && button.tooltip.is_empty());
    }
}
