//! The choice of watched panels: what is watched, how, and what the settings leave out.

use std::path::{Path, PathBuf};

use rc_vfs::{BackendId, VfsPath};

use super::{Mode, Shown, WatchSettings, excluded, local_list, mode, on_network};
use crate::DriveKind;

fn on() -> WatchSettings {
    WatchSettings {
        on: true,
        ..WatchSettings::default()
    }
}

fn files(path: &VfsPath) -> Shown<'_> {
    Shown {
        path,
        files: true,
        plain: true,
    }
}

#[test]
fn the_defaults_watch_nothing_but_a_prompt_still_reads() {
    let defaults = WatchSettings::default();
    assert!(!defaults.on && !defaults.changes && !defaults.network);
    assert!(defaults.background && defaults.exclude.is_empty());
    let local = VfsPath::local("/home/me/work");
    assert_eq!(mode(files(&local), false, &defaults), None);
    assert!(local_list(files(&local)));
}

#[test]
fn a_local_folder_is_watched_natively_a_wsl_share_polled() {
    let local = VfsPath::local("/home/me/work");
    assert_eq!(mode(files(&local), false, &on()), Some(Mode::Native));
    for wsl in [r"\\wsl.localhost\Ubuntu\home\me", r"\\wsl$\Ubuntu\home\me"] {
        let wsl = VfsPath::local(wsl);
        assert!(!on_network(&wsl, None));
        assert_eq!(mode(files(&wsl), false, &on()), Some(Mode::Poll));
    }
}

#[test]
fn a_terminal_a_flat_view_an_archive_a_server_and_a_device_are_not_watched() {
    let local = VfsPath::local("/home/me/work");
    let terminal = Shown {
        files: false,
        ..files(&local)
    };
    let flat = Shown {
        plain: false,
        ..files(&local)
    };
    for shown in [terminal, flat] {
        assert_eq!(mode(shown, false, &on()), None);
        assert!(!local_list(shown));
    }
    let backends = [
        BackendId::Archive {
            outer: Box::new(VfsPath::local("/home/me/a.zip")),
            format: rc_vfs::ArchiveFormat::Zip,
        },
        BackendId::Sftp { connection_id: 1 },
        BackendId::Ftp { connection_id: 1 },
        BackendId::Device {
            device: "phone".into(),
        },
    ];
    for backend in backends {
        let path = VfsPath {
            backend,
            path: PathBuf::from("/"),
        };
        assert_eq!(mode(files(&path), false, &on()), None, "{path:?}");
    }
}

#[test]
fn network_folders_only_with_their_switch() {
    let share = VfsPath::local(r"\\nas\media\films");
    assert!(on_network(&share, None));
    assert_eq!(mode(files(&share), true, &on()), None);
    let network = WatchSettings {
        network: true,
        ..on()
    };
    assert_eq!(mode(files(&share), true, &network), Some(Mode::Native));
    let mapped = VfsPath::local(r"Z:\films");
    assert!(on_network(&mapped, Some(DriveKind::Network)));
    assert!(!on_network(&mapped, Some(DriveKind::Fixed)));
}

#[test]
fn excluded_drives_are_letters_and_a_backslash_every_unc_path() {
    assert!(excluded(Path::new(r"D:\photos"), "D"));
    assert!(excluded(Path::new(r"d:\photos"), "CD"));
    assert!(excluded(Path::new(r"\\?\D:\photos"), "D"));
    assert!(!excluded(Path::new(r"C:\photos"), "D"));
    assert!(!excluded(Path::new(r"\\nas\media"), "D"));
    assert!(excluded(Path::new(r"\\nas\media"), "\\"));
    assert!(excluded(Path::new(r"\\?\UNC\nas\media"), "\\"));
    assert!(excluded(Path::new(r"\\wsl.localhost\Ubuntu\home"), "\\"));
    assert!(!excluded(Path::new("/home/me"), "D\\"));
    let settings = WatchSettings {
        exclude: "D\\".into(),
        network: true,
        ..on()
    };
    let photos = VfsPath::local(r"D:\photos");
    assert_eq!(mode(files(&photos), false, &settings), None);
    let share = VfsPath::local(r"\\nas\media");
    assert_eq!(mode(files(&share), true, &settings), None);
}
