use std::path::Path;

use russh_sftp::protocol::Status;

use super::*;

fn status(code: StatusCode) -> Error {
    Error::Status(Status {
        id: 1,
        status_code: code,
        error_message: "planted server text".to_owned(),
        language_tag: "en".to_owned(),
    })
}

#[test]
fn a_status_names_its_kind_and_never_carries_the_server_text() {
    let path = Path::new("/srv/a");
    let cases = [
        (StatusCode::NoSuchFile, "not_found"),
        (StatusCode::PermissionDenied, "permission_denied"),
        (StatusCode::OpUnsupported, "unsupported"),
        (StatusCode::BadMessage, "invalid_path"),
        (StatusCode::Failure, "io"),
    ];
    for (code, want) in cases {
        let error = into_vfs(status(code), path, 30);
        assert_eq!(error.code(), want, "{code:?}");
        assert!(!error.to_string().contains("planted"), "{error}");
    }
}

#[test]
fn a_session_failure_keeps_its_code_inside_the_file_system_error() {
    let path = Path::new("/srv");
    let cases = [
        (Error::Timeout, "sftp_timeout"),
        (
            Error::UnexpectedBehavior("session closed".into()),
            "connection_lost",
        ),
        (Error::IO("broken pipe".into()), "connection_lost"),
        (status(StatusCode::ConnectionLost), "connection_lost"),
        (Error::UnexpectedPacket, "sftp_protocol"),
    ];
    for (error, want) in cases {
        let error = into_vfs(error, path, 30);
        assert_eq!(error.code(), "io");
        let failure = SftpError::from_vfs(&error).expect("a session failure");
        assert_eq!(failure.code(), want);
        assert!(SftpError::ALL_CODES.contains(&failure.code()));
    }
    assert!(SftpError::from_vfs(&into_vfs(status(StatusCode::Failure), path, 30)).is_none());
}

#[test]
fn eof_and_no_such_file_are_recognised() {
    assert!(is_eof(&status(StatusCode::Eof)));
    assert!(!is_eof(&Error::Timeout));
    assert!(is_no_such_file(&status(StatusCode::NoSuchFile)));
    assert!(!is_no_such_file(&status(StatusCode::Failure)));
}
