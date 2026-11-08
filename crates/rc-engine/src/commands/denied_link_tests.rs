use std::path::PathBuf;

use rc_vfs::{BackendId, VfsError, VfsPath};

use super::*;

fn denied(path: &str) -> EngineError {
    EngineError::Vfs(VfsError::PermissionDenied {
        path: PathBuf::from(path),
    })
}

fn local(path: &str) -> VfsPath {
    VfsPath::local(PathBuf::from(path))
}

#[test]
fn a_denied_link_leads_to_its_target() {
    let link = local(r"C:\Users\me\Anwendungsdaten");
    let target = PathBuf::from(r"\\?\C:\Users\me\AppData\Roaming");
    assert_eq!(
        follow_target(&link, &denied(r"C:\Users\me\Anwendungsdaten"), Some(target)),
        Some(local(r"C:\Users\me\AppData\Roaming")),
        "the target as the panel shows it, without \\\\?\\"
    );
    assert_eq!(
        plain(&PathBuf::from(r"\\?\UNC\nas\share")),
        PathBuf::from(r"\\nas\share")
    );
}

#[test]
fn a_relative_target_is_taken_from_the_links_folder() {
    let link = local("/home/me/link");
    assert_eq!(
        follow_target(&link, &denied("/home/me/link"), Some(PathBuf::from("real"))),
        Some(VfsPath::local(PathBuf::from("/home/me").join("real")))
    );
}

#[test]
fn every_other_case_keeps_its_error() {
    let link = local("/home/me/link");
    let target = || Some(PathBuf::from("/srv/real"));
    // A plain folder that is denied: no link to follow.
    assert_eq!(follow_target(&link, &denied("/home/me/link"), None), None);
    // A dangling link: not found, not denied.
    let gone = EngineError::Vfs(VfsError::NotFound {
        path: PathBuf::from("/home/me/link"),
    });
    assert_eq!(follow_target(&link, &gone, target()), None);
    // Not on this machine.
    let server = VfsPath {
        backend: BackendId::Sftp { connection_id: 1 },
        path: PathBuf::from("/home/me/link"),
    };
    assert_eq!(
        follow_target(&server, &denied("/home/me/link"), target()),
        None
    );
    // A link to itself.
    assert_eq!(
        follow_target(
            &link,
            &denied("/home/me/link"),
            Some(PathBuf::from("/home/me/link"))
        ),
        None
    );
}

/// On Windows a junction's target is read and offered for a refused listing.
#[cfg(windows)]
#[tokio::test]
async fn a_junction_is_followed_on_windows() {
    use rc_vfs::{LinkKind, LocalFs, Vfs};

    let dir = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let target = dir.path().join("Roaming");
    fs.mkdir(&target, false).await.expect("mkdir");
    let junction = dir.path().join("Anwendungsdaten");
    fs.create_link(&junction, &target, LinkKind::Junction)
        .await
        .expect("junction");
    let engine = crate::test_support::headless(dir.path()).await;
    let link = VfsPath::local(junction.clone());
    let followed = engine
        .denied_link_target(&link, &denied(&junction.to_string_lossy()))
        .await;
    assert_eq!(followed, Some(VfsPath::local(target)));
    // A plain folder is no link: its error stays.
    let plain_dir = VfsPath::local(dir.path().to_path_buf());
    assert_eq!(
        engine
            .denied_link_target(&plain_dir, &denied(&dir.path().to_string_lossy()))
            .await,
        None
    );
}
