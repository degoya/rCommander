//! `sftp.upload_rights` (RC-080-67): the rights a completed upload gets, read when the backend
//! opens; an upload is owner-only until then.

use std::{os::unix::fs::PermissionsExt as _, path::Path};

use rc_vfs::{Vfs, WriteMode};
use serde_json::json;
use tokio::io::AsyncWriteExt;

use super::tests::harness;
use super::*;
use crate::{Command, settings::SFTP_UPLOAD_RIGHTS_KEY};

#[test]
fn the_setting_reads_as_octal_rights() {
    assert_eq!(upload_mode(Some("644")), 0o644);
    assert_eq!(upload_mode(Some("664")), 0o664);
    assert_eq!(upload_mode(Some("600")), 0o600);
    for broken in [None, Some("rw"), Some("7777"), Some("")] {
        assert_eq!(upload_mode(broken), 0o644, "{broken:?}");
    }
}

#[tokio::test]
async fn a_completed_upload_gets_the_rights_of_the_setting() {
    let harness = harness().await;
    harness
        .engine
        .dispatch(Command::SetSetting {
            key: SFTP_UPLOAD_RIGHTS_KEY.into(),
            value: json!("664"),
        })
        .await
        .expect("set");
    let vfs = harness
        .engine
        .open_sftp(harness.id, true)
        .await
        .expect("opens");
    let mut stream = vfs
        .open_write(Path::new("/new.txt"), WriteMode::CreateNew)
        .await
        .expect("open");
    stream.write_all(b"x").await.expect("write");
    stream.shutdown().await.expect("done");
    let mode = std::fs::metadata(harness.served.path().join("new.txt"))
        .expect("metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o664);
}
