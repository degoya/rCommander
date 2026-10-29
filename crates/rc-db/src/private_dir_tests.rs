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
