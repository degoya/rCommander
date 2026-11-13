use std::io;

use super::in_use;

/// Access denied is "open elsewhere" on Windows only (RC-110-26): on Linux it is a folder that
/// may not be written to, reported as such at once instead of after five tries.
#[test]
fn access_denied_means_in_use_on_windows_only() {
    let denied = io::Error::from(io::ErrorKind::PermissionDenied);
    assert_eq!(in_use(&denied), cfg!(windows));
    // `ERROR_SHARING_VIOLATION` is Windows' number; elsewhere 32 is a broken pipe.
    assert_eq!(in_use(&io::Error::from_raw_os_error(32)), cfg!(windows));
    assert!(!in_use(&io::Error::from(io::ErrorKind::NotFound)));
}
