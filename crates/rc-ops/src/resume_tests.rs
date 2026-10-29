use std::{io, time::Duration};

use rc_vfs::VfsError;

use super::{RetryPolicy, is_connection_lost};
use crate::OpsError;

fn io_error(kind: io::ErrorKind) -> OpsError {
    OpsError::from(VfsError::Io {
        path: "/srv/big.iso".into(),
        source: io::Error::new(kind, "test"),
    })
}

#[test]
fn a_broken_link_is_a_lost_connection_and_a_refusal_is_not() {
    for kind in [
        io::ErrorKind::ConnectionAborted,
        io::ErrorKind::ConnectionReset,
        io::ErrorKind::BrokenPipe,
        io::ErrorKind::TimedOut,
        io::ErrorKind::UnexpectedEof,
        io::ErrorKind::NetworkUnreachable,
    ] {
        assert!(is_connection_lost(&io_error(kind)), "{kind:?}");
    }
    assert!(!is_connection_lost(&io_error(io::ErrorKind::Other)));
    assert!(!is_connection_lost(&OpsError::from(VfsError::NotFound {
        path: "/a".into()
    })));
    assert!(!is_connection_lost(&OpsError::DiskFull {
        path: "/a".into()
    }));
}

#[test]
fn the_wait_doubles_from_the_delay() {
    let policy = RetryPolicy {
        attempts: 3,
        delay: Duration::from_secs(5),
    };
    assert_eq!(policy.wait(0), Duration::from_secs(5));
    assert_eq!(policy.wait(1), Duration::from_secs(10));
    assert_eq!(policy.wait(2), Duration::from_secs(20));
    assert_eq!(RetryPolicy::default(), policy);
    // Large counts saturate instead of overflowing.
    assert!(policy.wait(u32::MAX) >= policy.wait(16));
}
