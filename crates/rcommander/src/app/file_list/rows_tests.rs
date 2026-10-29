use rc_vfs::{Attributes, Entry, EntryKind, WindowsFlags};

use iced::Color;

use super::{Column, is_hidden, value_colour};

fn entry(hidden: bool, windows: Option<WindowsFlags>) -> Entry {
    Entry {
        name: "pagefile.sys".into(),
        kind: EntryKind::File,
        size: 1,
        modified: None,
        created: None,
        attributes: Attributes {
            windows,
            ..Attributes::default()
        },
        hidden,
        owner: None,
    }
}

/// Total Commander marks hidden and system entries with `!` (owner, 2026-10-19): a dot file,
/// the hidden attribute and the system attribute all count; a plain file does not.
#[test]
fn hidden_and_system_entries_are_marked() {
    assert!(is_hidden(&entry(true, None)));
    let system = WindowsFlags {
        system: true,
        ..WindowsFlags::default()
    };
    assert!(is_hidden(&entry(false, Some(system))));
    let hidden = WindowsFlags {
        hidden: true,
        ..WindowsFlags::default()
    };
    assert!(is_hidden(&entry(false, Some(hidden))));
    assert!(!is_hidden(&entry(false, Some(WindowsFlags::default()))));
    assert!(!is_hidden(&entry(false, None)));
}

/// A rule by file type colours the extension with the name; the other columns and a marked row
/// keep the quieter colour.
#[test]
fn the_extension_takes_the_rule_s_colour() {
    let (red, grey) = (
        Color::from_rgb8(0xc4, 0x2b, 0x1c),
        Color::from_rgb8(0x61, 0x61, 0x61),
    );
    assert_eq!(value_colour(Column::Ext, Some(red), false, grey), red);
    assert_eq!(value_colour(Column::Size, Some(red), false, grey), grey);
    assert_eq!(value_colour(Column::Ext, None, false, grey), grey);
    assert_eq!(value_colour(Column::Ext, Some(red), true, grey), grey);
}
