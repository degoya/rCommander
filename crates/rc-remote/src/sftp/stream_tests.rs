use std::{
    io,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use rc_vfs::VfsError;

use super::typed_failure;
use crate::sftp::SftpError;

#[test]
fn a_lost_session_mid_stream_is_connection_lost_and_closes_the_backend() {
    let lost = AtomicBool::new(false);
    let error = typed_failure(
        Some(SftpError::ConnectionLost),
        Path::new("/big.iso"),
        &lost,
    )
    .expect("typed");
    assert!(lost.load(Ordering::SeqCst));
    match VfsError::from_io(error, "/elsewhere") {
        VfsError::Io { path, source } => {
            assert_eq!(path, Path::new("/big.iso"));
            assert_eq!(source.kind(), io::ErrorKind::ConnectionAborted);
            let typed = VfsError::Io { path, source };
            let inner = SftpError::from_vfs(&typed);
            assert_eq!(inner.map(SftpError::code), Some("connection_lost"));
        }
        other => panic!("expected an I/O error, got {other:?}"),
    }
}

#[test]
fn a_stalled_request_is_a_timeout() {
    let lost = AtomicBool::new(false);
    let error = typed_failure(
        Some(SftpError::Timeout { seconds: 30 }),
        Path::new("/a"),
        &lost,
    )
    .expect("typed");
    assert!(lost.load(Ordering::SeqCst));
    let typed = VfsError::from_io(error, "/a");
    assert!(
        matches!(&typed, VfsError::Io { source, .. } if source.kind() == io::ErrorKind::TimedOut)
    );
}

#[test]
fn other_failures_pass_unchanged() {
    let lost = AtomicBool::new(false);
    let protocol = SftpError::Protocol { reason: "test" };
    assert!(typed_failure(Some(protocol), Path::new("/a"), &lost).is_none());
    assert!(typed_failure(None, Path::new("/a"), &lost).is_none());
    assert!(!lost.load(Ordering::SeqCst));
}
