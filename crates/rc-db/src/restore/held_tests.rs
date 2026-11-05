use std::io;

use super::{digest, still};

async fn held(path: &std::path::Path) -> tokio::fs::File {
    tokio::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .await
        .expect("open")
}

/// RC-080-68: a name is the checked file's only while the held file is unchanged - a change
/// written through the name is refused.
#[tokio::test]
async fn a_name_is_taken_only_while_it_holds_the_checked_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("rcommander.db.restore");
    std::fs::write(&path, b"checked bytes").expect("write");
    let mut file = held(&path).await;
    let expected = digest(&mut file).await.expect("digest");
    still(&mut file, &path, expected).await.expect("unchanged");
    std::fs::write(&path, b"changed bytes").expect("write");
    let changed = still(&mut file, &path, expected).await;
    assert_eq!(
        changed.err().map(|error| error.kind()),
        Some(io::ErrorKind::InvalidData)
    );
}

/// RC-080-68: another file renamed over the name, or a link to the held file, is not its name.
#[cfg(unix)]
#[tokio::test]
async fn a_replaced_or_linked_name_is_not_the_held_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("rcommander.db.restore");
    std::fs::write(&path, b"checked bytes").expect("write");
    let mut file = held(&path).await;
    let expected = digest(&mut file).await.expect("digest");
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&path, &link).expect("link");
    assert!(still(&mut file, &link, expected).await.is_err());

    let other = dir.path().join("other");
    std::fs::write(&other, b"checked bytes").expect("write");
    std::fs::rename(&other, &path).expect("swap");
    let swapped = still(&mut file, &path, expected).await;
    assert_eq!(
        swapped.err().map(|error| error.kind()),
        Some(io::ErrorKind::InvalidData)
    );
}

/// The copy keeps the permissions of the file it is written into, not the source's.
#[cfg(unix)]
#[tokio::test]
async fn a_copy_keeps_the_targets_permissions() {
    use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};

    use super::copy_into;
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("source");
    std::fs::write(&source, b"backup bytes").expect("write");
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o644)).expect("chmod");
    let target = dir.path().join("target");
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&target)
        .expect("create");
    let mut file = tokio::fs::File::from_std(file);
    copy_into(&source, &mut file).await.expect("copied");
    assert_eq!(std::fs::read(&target).expect("read"), b"backup bytes");
    let mode = std::fs::metadata(&target)
        .expect("meta")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    assert!(copy_into(dir.path(), &mut file).await.is_err(), "a folder");
}
