use std::path::{Path, PathBuf};

use super::{breadcrumb, look, remote_breadcrumb};
use crate::theme;

fn labels(path: &str) -> Vec<(String, PathBuf)> {
    breadcrumb(Path::new(path))
}

#[cfg(not(windows))]
#[test]
fn every_segment_leads_to_its_directory() {
    assert_eq!(
        labels("/home/user/docs"),
        [
            ("/".to_owned(), PathBuf::from("/")),
            ("home".to_owned(), PathBuf::from("/home")),
            ("user".to_owned(), PathBuf::from("/home/user")),
            ("docs".to_owned(), PathBuf::from("/home/user/docs")),
        ]
    );
    assert_eq!(labels("/"), [("/".to_owned(), PathBuf::from("/"))]);
}

#[cfg(windows)]
#[test]
fn a_drive_and_a_share_are_one_segment_with_their_root() {
    assert_eq!(
        labels(r"C:\Users\me"),
        [
            (r"C:\".to_owned(), PathBuf::from(r"C:\")),
            ("Users".to_owned(), PathBuf::from(r"C:\Users")),
            ("me".to_owned(), PathBuf::from(r"C:\Users\me")),
        ]
    );
    let share = labels(r"\\wsl.localhost\Ubuntu\home");
    assert_eq!(share[0].0, r"\\wsl.localhost\Ubuntu\");
    assert_eq!(share[1].1, PathBuf::from(r"\\wsl.localhost\Ubuntu\home"));
}

#[test]
fn a_server_path_starts_with_its_location_on_every_platform() {
    let crumbs = remote_breadcrumb("sftp://box", Path::new("/home/tester"));
    assert_eq!(
        crumbs,
        [
            ("sftp://box/".to_owned(), PathBuf::from("/")),
            ("home".to_owned(), PathBuf::from("/home")),
            ("tester".to_owned(), PathBuf::from("/home/tester")),
        ]
    );
    assert_eq!(
        remote_breadcrumb("sftp://box", Path::new("/")),
        [("sftp://box/".to_owned(), PathBuf::from("/"))]
    );
}

/// RC-070-03: a panel on an FTP server writes its location with the connection's name, as the
/// drive bar knows it - what the path prompt starts with and takes back.
#[tokio::test]
async fn an_ftp_location_is_written_with_its_connection_name() {
    use rc_engine::{Drive, DriveGroup, DriveKind, DriveState, VfsPath};
    use rc_vfs::BackendId;

    use crate::app::{Message, drive_bar::DriveMessage, harness::Harness};

    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::headless(root.path()).await;
    let server = Drive {
        id: "server:5".to_owned(),
        group: DriveGroup::Servers,
        kind: DriveKind::Server,
        label: "nas".to_owned(),
        root: Some(VfsPath {
            backend: BackendId::Ftp { connection_id: 5 },
            path: "/".into(),
        }),
        state: DriveState::Unknown,
        volume: None,
        space: None,
        folder: Vec::new(),
        favourite: true,
        remote: None,
    };
    harness
        .send(Message::Drive(DriveMessage::Listed(Ok(vec![server]))))
        .await;
    let at = |path: &str| VfsPath {
        backend: BackendId::Ftp { connection_id: 5 },
        path: path.into(),
    };
    assert_eq!(
        harness.app.location_text(&at("/pub/My Files")),
        "ftp://nas/pub/My Files"
    );
    assert_eq!(harness.app.location_text(&at("/")), "ftp://nas/");
}

#[test]
fn the_active_path_changes_its_colour_and_keeps_its_font() {
    let list = theme::fonts::list();
    let tokens = theme::tokens(true);
    let (active_font, active_colour) = look(list, true, tokens);
    let (other_font, other_colour) = look(list, false, tokens);
    assert_eq!(active_font, list.font);
    assert_eq!(other_font, list.font);
    assert_ne!(active_colour, other_colour);
}
