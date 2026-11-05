//! An upload's rights (RC-080-67): the file is created owner-only, whatever the server's umask,
//! and gets `SftpLimits::upload_mode` only once it is complete; an existing file keeps its rights.

use std::{os::unix::fs::PermissionsExt as _, path::Path};

use rc_vfs::{Vfs, WriteMode};
use tokio::io::AsyncWriteExt;

use super::{SftpLimits, UPLOAD_MODE, tests::Fixture};

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path)
        .expect("metadata")
        .permissions()
        .mode()
        & 0o7777
}

#[tokio::test]
async fn an_upload_is_owner_only_until_it_is_complete() {
    let fixture = Fixture::start(|_| {}).await;
    let (vfs, _session) = fixture.vfs().await;
    let mut stream = vfs
        .open_write(Path::new("/secret.txt"), WriteMode::CreateNew)
        .await
        .expect("open");
    stream.write_all(b"half of it").await.expect("write");
    stream.flush().await.expect("flush");
    assert_eq!(mode(&fixture.local("secret.txt")), UPLOAD_MODE, "during");
    stream.shutdown().await.expect("done");
    assert_eq!(
        mode(&fixture.local("secret.txt")),
        crate::sftp::DEFAULT_UPLOAD_MODE,
        "complete"
    );

    // An upload that breaks off leaves its part owner-only.
    let mut stream = vfs
        .open_write(Path::new("/broken.part"), WriteMode::CreateNew)
        .await
        .expect("open");
    stream.write_all(b"part").await.expect("write");
    drop(stream);
    assert_eq!(mode(&fixture.local("broken.part")), UPLOAD_MODE);

    // A file it truncates keeps its rights; `preserve` sets the final ones.
    std::fs::write(fixture.local("shared.txt"), b"old").expect("write");
    std::fs::set_permissions(
        fixture.local("shared.txt"),
        std::fs::Permissions::from_mode(0o640),
    )
    .expect("chmod");
    let mut stream = vfs
        .open_write(Path::new("/shared.txt"), WriteMode::Truncate)
        .await
        .expect("open");
    stream.write_all(b"new").await.expect("write");
    stream.shutdown().await.expect("done");
    assert_eq!(mode(&fixture.local("shared.txt")), 0o640);
}

#[tokio::test]
async fn an_owner_only_setting_keeps_the_upload_owner_only() {
    let fixture = Fixture::start(|_| {}).await;
    let (vfs, _session) = fixture
        .vfs_with(SftpLimits {
            upload_mode: UPLOAD_MODE,
            ..SftpLimits::default()
        })
        .await;
    rc_vfs::conformance::write_file(&vfs, Path::new("/a.txt"), WriteMode::CreateNew, b"abc").await;
    assert_eq!(mode(&fixture.local("a.txt")), UPLOAD_MODE);
}
