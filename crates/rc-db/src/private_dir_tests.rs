#[cfg(unix)]
use super::create_private_dir;

/// RC-080-42: a new data folder and its new parents are owner-only; the platform's data folder
/// is tightened when it was there before, a folder elsewhere is not.
#[cfg(unix)]
#[tokio::test]
async fn the_data_folder_is_the_users_alone() {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = |path: &std::path::Path| {
        std::fs::metadata(path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777
    };
    let root = tempfile::tempdir().expect("temp dir");
    let new = root.path().join("a").join("b");
    create_private_dir(&new, None).await.expect("created");
    assert_eq!(mode(&new), 0o700);
    assert_eq!(mode(&root.path().join("a")), 0o700);

    let open = root.path().join("open");
    std::fs::create_dir(&open).expect("mkdir");
    std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    create_private_dir(&open, None).await.expect("there");
    assert_eq!(mode(&open), 0o755, "a folder the person named stays");
    create_private_dir(&open, Some(&open)).await.expect("there");
    assert_eq!(mode(&open), 0o700, "the data folder is tightened");
}

/// RC-080-68: the database and SQLite's files beside it are the user's alone - a new database
/// from its creation, an old one left open to others from the next start. A link is not
/// followed.
#[cfg(unix)]
#[tokio::test]
async fn the_database_files_are_the_users_alone() {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = |path: &std::path::Path| {
        std::fs::metadata(path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777
    };
    let beside = |path: &std::path::Path, suffix: &str| {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        std::path::PathBuf::from(name)
    };
    let root = tempfile::tempdir().expect("temp dir");
    let path = root.path().join(crate::DB_FILE);
    let db = crate::Db::open(&path).await.expect("open");
    db.set("state", "written").await.expect("set");
    for file in [path.clone(), beside(&path, "-wal"), beside(&path, "-shm")] {
        assert_eq!(mode(&file), 0o600, "{}", file.display());
    }
    db.close().await;

    for file in [path.clone(), beside(&path, "-wal")] {
        if file.exists() {
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).expect("chmod");
        }
    }
    let db = crate::Db::open(&path).await.expect("reopen");
    assert_eq!(mode(&path), 0o600, "tightened at the start");
    db.close().await;

    let target = root.path().join("elsewhere");
    std::fs::write(&target, b"").expect("write");
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).expect("chmod");
    let link = root.path().join("linked.db");
    std::os::unix::fs::symlink(&target, &link).expect("link");
    super::secure_database_files(&link)
        .await
        .expect("left alone");
    assert_eq!(mode(&target), 0o644, "a link is not followed");
}
