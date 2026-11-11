use super::*;
use crate::{BarKind, Command, Keymap};

#[test]
fn the_table_is_sorted_lower_case_and_unique() {
    let names: Vec<&str> = TC_NAMES.iter().map(|(tc, _)| *tc).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(names, sorted, "sorted and once each, for the binary search");
    for name in names {
        assert!(name.starts_with("cm_"), "{name}");
        assert_eq!(name, name.to_ascii_lowercase(), "{name}");
    }
}

/// Every row the table names is one the standard keymap has - a binding or a window key - or a
/// bar's palette row.
#[test]
fn cm_names_map_to_rows() {
    let standard = Keymap::shared_standard();
    for (tc, row) in TC_NAMES {
        let known = standard
            .bindings()
            .iter()
            .any(|binding| binding.name == *row)
            || standard.window().iter().any(|key| key.name == *row)
            || BarKind::of_toggle_row(row).is_some();
        assert!(known, "{tc} maps to {row}, which the keymap does not have");
    }
    assert_eq!(row_of("cm_Copy"), Some("copy"));
    assert_eq!(row_of("CM_LIST"), Some("view_file"));
    assert_eq!(row_of(" cm_RenMov "), Some("move"));
    assert_eq!(row_of("cm_NoSuchCommand"), None);
    assert_eq!(row_of("copy"), None);
}

/// RC-110-15: Total Commander's `cm_VisButtonBar` and `cm_VisButtonBar2` show and hide the two
/// button bars, through the rows the palette and the View menu run; no key runs them.
#[test]
fn the_button_bars_switch_by_their_total_commander_names() {
    let row = row_of("cm_VisButtonBar").expect("known");
    assert_eq!(row, "toggle_bar.button_bar");
    assert_eq!(
        BarKind::of_toggle_row(row).map(BarKind::toggle_command),
        Some(Command::ToggleButtonBar {})
    );
    let row = row_of("CM_VISBUTTONBAR2").expect("known");
    assert_eq!(
        BarKind::of_toggle_row(row).map(BarKind::toggle_command),
        Some(Command::ToggleVerticalButtonBar {})
    );
    for bar in BarKind::ALL {
        assert_eq!(BarKind::of_toggle_row(&bar.toggle_row()), Some(bar));
    }
    assert_eq!(
        BarKind::DriveBar.toggle_command(),
        Command::ToggleBar {
            bar: BarKind::DriveBar
        }
    );
    assert_eq!(BarKind::of_toggle_row("toggle_bar.nothing"), None);
    let standard = Keymap::shared_standard();
    for id in ["view.toggle_button_bar", "view.toggle_vertical_button_bar"] {
        assert!(
            standard
                .bindings()
                .iter()
                .all(|binding| binding.command.id() != id),
            "{id} has no key, as in Total Commander"
        );
    }
}
