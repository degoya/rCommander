use std::{fs, io};

use super::{BoundedReadError, read_bounded};

#[test]
fn a_file_within_the_limit_is_read_whole() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("fits");
    fs::write(&file, b"0123456789").expect("write");
    assert_eq!(
        read_bounded(&file, 10).expect("reads").as_slice(),
        b"0123456789"
    );
    let empty = dir.path().join("empty");
    fs::write(&empty, b"").expect("write");
    assert!(read_bounded(&empty, 0).expect("reads").is_empty());
}

#[test]
fn a_larger_file_a_folder_and_a_missing_one_are_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("large");
    fs::write(&file, b"0123456789!").expect("write");
    assert!(matches!(
        read_bounded(&file, 10),
        Err(BoundedReadError::TooLarge { max: 10 })
    ));
    assert!(matches!(
        read_bounded(dir.path(), 10),
        Err(BoundedReadError::NotAFile)
    ));
    assert!(matches!(
        read_bounded(&dir.path().join("missing"), 10),
        Err(BoundedReadError::Io(error)) if error.kind() == io::ErrorKind::NotFound
    ));
}

#[test]
fn as_an_io_error_each_failure_keeps_its_kind() {
    let kind = |error: BoundedReadError| io::Error::from(error).kind();
    assert_eq!(
        kind(BoundedReadError::NotAFile),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        kind(BoundedReadError::TooLarge { max: 1 }),
        io::ErrorKind::FileTooLarge
    );
    let denied = io::Error::from(io::ErrorKind::PermissionDenied);
    assert_eq!(
        kind(BoundedReadError::Io(denied)),
        io::ErrorKind::PermissionDenied
    );
}
