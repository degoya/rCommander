use std::collections::BTreeSet;

use rc_engine::{Drive, DriveGroup, DriveKind, DriveSpace, DriveState, PanelId, VfsPath};

use super::{
    MORE, SMALL, display_name, human_size, item_width, kind_symbol, line_text, short_label, shown,
    tooltip_text,
};
use crate::app::drive_bar::{
    menu::{DriveMenu, MenuLine, Scope, lines},
    overflow::{self, BarItem, bar_items},
};

fn drive(label: &str, kind: DriveKind, state: DriveState) -> Drive {
    Drive {
        id: label.to_owned(),
        group: DriveGroup::Drives,
        kind,
        label: label.to_owned(),
        root: Some(VfsPath::local("/")),
        state,
        volume: None,
        space: None,
        folder: Vec::new(),
        favourite: false,
        folder_buttons: Vec::new(),
        remote: None,
    }
}

#[test]
fn sizes_read_as_explorer_writes_them() {
    assert_eq!(human_size(512), "512 B");
    assert_eq!(human_size(1536), "1.5 KiB");
    assert_eq!(human_size(5 * 1024 * 1024 * 1024), "5.0 GiB");
    assert_eq!(human_size(3 * 1024 * 1024 * 1024 * 1024), "3.0 TiB");
}

#[test]
fn every_kind_has_its_own_symbol() {
    let kinds = [
        DriveKind::Fixed,
        DriveKind::Removable,
        DriveKind::Network,
        DriveKind::Optical,
        DriveKind::Ram,
        DriveKind::Unknown,
        DriveKind::Wsl,
        DriveKind::Server,
    ];
    let symbols: BTreeSet<&str> = kinds.iter().map(|kind| kind_symbol(*kind)).collect();
    assert_eq!(symbols.len(), kinds.len());
}

#[test]
fn a_drive_letter_shows_without_its_colon_a_name_as_it_is() {
    assert_eq!(
        short_label(&drive("C:", DriveKind::Fixed, DriveState::Ready)),
        "C"
    );
    assert_eq!(
        short_label(&drive("/", DriveKind::Fixed, DriveState::Ready)),
        "/"
    );
    let mut distro = drive("Ubuntu:", DriveKind::Wsl, DriveState::Unknown);
    distro.group = DriveGroup::Wsl;
    assert_eq!(short_label(&distro), "Ubuntu:");
}

#[test]
fn the_tooltip_tells_the_volume_and_the_free_space_or_why_there_is_none() {
    let mut ready = drive("C:", DriveKind::Fixed, DriveState::Ready);
    ready.volume = Some("System".into());
    ready.space = Some(DriveSpace {
        free: 2 * 1024 * 1024 * 1024,
        total: 8 * 1024 * 1024 * 1024,
    });
    assert_eq!(display_name(&ready), "System (C:)");
    assert_eq!(line_text(&ready), "2.0 GiB free of 8.0 GiB");
    assert_eq!(tooltip_text(&ready), "System (C:): 2.0 GiB free of 8.0 GiB");

    let empty = drive("D:", DriveKind::Optical, DriveState::NotReady);
    assert_eq!(tooltip_text(&empty), "D:: Not ready");
    let asked = drive("E:", DriveKind::Removable, DriveState::Waiting);
    assert_eq!(line_text(&asked), "Waiting for the drive");
    let mut share = drive("N:", DriveKind::Network, DriveState::Unknown);
    assert_eq!(tooltip_text(&share), "N:");
    // A mapped network drive is named by its folder, even once its volume answered.
    share.remote = Some(r"\\nas\media".to_owned());
    share.volume = Some("DATA".to_owned());
    assert_eq!(tooltip_text(&share), r"\\nas\media (N:)");

    let mut server = drive("web", DriveKind::Server, DriveState::Unknown);
    server.group = DriveGroup::Servers;
    for backend in [
        rc_vfs::BackendId::Sftp { connection_id: 1 },
        rc_vfs::BackendId::Ftp { connection_id: 1 },
    ] {
        server.root = Some(VfsPath {
            backend,
            path: "/".into(),
        });
        assert_eq!(
            tooltip_text(&server),
            "web",
            "a server is named, SFTP or FTP"
        );
    }
}

/// Three drives, a distribution, and the servers `a` and `b` in the folder `Work`.
fn bar_drives() -> Vec<Drive> {
    let mut drives: Vec<Drive> = ["C:", "D:", "E:"]
        .into_iter()
        .map(|label| drive(label, DriveKind::Fixed, DriveState::Ready))
        .collect();
    let mut distro = drive("Debian", DriveKind::Wsl, DriveState::Unknown);
    distro.group = DriveGroup::Wsl;
    drives.push(distro);
    for name in ["a", "b"] {
        let mut server = drive(name, DriveKind::Server, DriveState::Unknown);
        server.group = DriveGroup::Servers;
        server.folder = vec!["Work".to_owned()];
        server.folder_buttons = vec![1];
        drives.push(server);
    }
    drives
}

/// The owner's report (2026-11-01): `»` opened the whole drive menu. It lists exactly the items
/// the bar cut off - the bar draws `items[..shown]` and sends `shown` - in the bar's order.
#[test]
fn the_overflow_menu_holds_exactly_what_the_bar_cut_off() {
    let drives = bar_drives();
    let items = bar_items(&drives);
    let work = ["Work".to_owned()];
    assert_eq!(
        items,
        [
            BarItem::Drive(0),
            BarItem::Drive(1),
            BarItem::Drive(2),
            BarItem::Gap,
            BarItem::Drive(3),
            BarItem::Gap,
            BarItem::Folder(&work),
        ]
    );
    let widths: Vec<f32> = items
        .iter()
        .map(|item| item_width(&drives, *item))
        .collect();
    let more = overflow::button_width(&[MORE], SMALL);
    // Room for C, D and E, then `»` (and half a pixel against rounding).
    let width = widths[..3].iter().sum::<f32>() + 3.0 * overflow::SPACING + more + 0.5;
    let cut = shown(&drives, &items, width);
    assert_eq!(cut, 3);

    let menu = DriveMenu::scoped(PanelId::Left, Scope::Overflow(cut), &drives, None);
    let listed: Vec<MenuLine<'_>> = lines(&drives, &menu);
    assert_eq!(
        listed,
        [
            MenuLine::Entry {
                stop: 0,
                drive: 3,
                indent: false
            },
            MenuLine::Folder(&work),
            MenuLine::Entry {
                stop: 1,
                drive: 4,
                indent: true
            },
            MenuLine::Entry {
                stop: 2,
                drive: 5,
                indent: true
            },
        ],
        "Debian, then the folder Work with its servers; nothing the bar shows"
    );

    let all = widths.iter().sum::<f32>() + overflow::SPACING * (widths.len() - 1) as f32;
    assert_eq!(shown(&drives, &items, all), items.len(), "everything fits");
    let empty = DriveMenu::scoped(PanelId::Left, Scope::Overflow(items.len()), &drives, None);
    assert!(lines(&drives, &empty).is_empty());
}
