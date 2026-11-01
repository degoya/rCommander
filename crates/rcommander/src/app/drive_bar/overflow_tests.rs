use rc_engine::{Drive, DriveGroup, DriveKind, DriveState};

use super::{BarItem, SPACING, bar_items, button_width, visible_count};

/// A server in `folder`, under the folder buttons `buttons`, marked or not.
fn server(label: &str, folder: &[&str], buttons: &[usize], favourite: bool) -> Drive {
    Drive {
        id: format!("server:{label}"),
        group: DriveGroup::Servers,
        kind: DriveKind::Server,
        label: label.to_owned(),
        root: None,
        state: DriveState::Unknown,
        volume: None,
        space: None,
        folder: folder.iter().map(|name| (*name).to_owned()).collect(),
        folder_buttons: buttons.to_vec(),
        favourite,
        remote: None,
    }
}

#[test]
fn a_bar_that_fits_shows_every_button_and_no_overflow() {
    let widths = [40.0, 40.0, 40.0];
    assert_eq!(visible_count(&widths, 120.0 + 2.0 * SPACING, 20.0), 3);
    assert_eq!(visible_count(&widths, 500.0, 20.0), 3);
    assert_eq!(visible_count(&[], 0.0, 20.0), 0);
}

#[test]
fn a_bar_too_narrow_keeps_the_first_buttons_and_room_for_the_overflow() {
    let widths = [40.0, 40.0, 40.0, 40.0];
    // 40 + 3 + 40 = 83, then 3 and the 20-wide overflow: 106.
    assert_eq!(visible_count(&widths, 106.0, 20.0), 2);
    assert_eq!(visible_count(&widths, 105.0, 20.0), 1);
    assert_eq!(visible_count(&widths, 150.0, 20.0), 3);
    assert_eq!(visible_count(&widths, 10.0, 20.0), 0);
}

#[test]
fn a_wider_label_makes_a_wider_button() {
    let short = button_width(&["\u{1f5b4}", "C"], 12.0);
    let long = button_width(&["\u{1f5b4}", "Ubuntu-24.04"], 12.0);
    assert!(long > short + 40.0, "{short} {long}");
    assert!(short > 16.0, "padding and symbol: {short}");
}

/// RC-100-63: a marked subfolder is a button of its own after its parent's, an unmarked
/// top-level folder is none, and the bar keeps the order `drives.list` gives (the tree's).
#[test]
fn the_bar_shows_a_button_per_marked_folder_in_the_tree_s_order() {
    let drives = vec![
        server("deep", &["Work", "Staging"], &[1, 2], false),
        server("web", &["Work"], &[1], false),
        server("old", &["Archive"], &[], false),
        server("kept", &["Archive", "Keep"], &[2], false),
        server("jump", &[], &[], true),
    ];
    let work = ["Work".to_owned()];
    let staging = ["Work".to_owned(), "Staging".to_owned()];
    let keep = ["Archive".to_owned(), "Keep".to_owned()];
    assert_eq!(
        bar_items(&drives),
        [
            BarItem::Folder(&work),
            BarItem::Folder(&staging),
            BarItem::Folder(&keep),
            BarItem::Drive(4),
        ]
    );
}

#[test]
fn a_bar_without_marked_folders_shows_only_the_marked_servers() {
    let drives = vec![
        server("a", &["Work"], &[], false),
        server("b", &["Work"], &[], true),
        // A length past the path names no folder.
        server("c", &["Home"], &[3], false),
    ];
    assert_eq!(bar_items(&drives), [BarItem::Drive(1)]);
}
