use std::collections::BTreeSet;

use rc_engine::{Drive, DriveGroup, DriveKind, DriveSpace, DriveState, VfsPath};

use super::{display_name, human_size, kind_symbol, line_text, short_label, tooltip_text};

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
