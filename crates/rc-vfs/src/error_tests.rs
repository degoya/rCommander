use std::io;

use super::VfsError;

fn mapped(kind: io::ErrorKind) -> VfsError {
    VfsError::from_io(io::Error::from(kind), "/x")
}

#[test]
fn io_kinds_map_to_their_variants_and_codes() {
    let cases = [
        (io::ErrorKind::NotFound, "not_found"),
        (io::ErrorKind::PermissionDenied, "permission_denied"),
        (io::ErrorKind::AlreadyExists, "already_exists"),
        (io::ErrorKind::NotADirectory, "not_a_directory"),
        (io::ErrorKind::IsADirectory, "is_a_directory"),
        (io::ErrorKind::DirectoryNotEmpty, "directory_not_empty"),
        (io::ErrorKind::Unsupported, "unsupported"),
        (io::ErrorKind::TimedOut, "io"),
    ];
    for (kind, code) in cases {
        assert_eq!(mapped(kind).code(), code, "{kind:?}");
    }
}

#[test]
fn the_message_names_the_path() {
    let error = mapped(io::ErrorKind::NotFound);
    assert_eq!(error.to_string(), "/x: not found");
}

#[test]
fn an_unmapped_error_keeps_its_source() {
    let error = VfsError::from_io(io::Error::other("disk on fire"), "/x");
    assert!(matches!(&error, VfsError::Io { source, .. } if source.to_string() == "disk on fire"));
    assert_eq!(error.code(), "io");
}

/// A stream reports its failure as an I/O error; mapped back, it keeps its code and path.
#[test]
fn a_wrapped_error_comes_back_intact() {
    let limit = VfsError::ArchiveLimit {
        path: "/inner/entry".into(),
        limit: "compression ratio",
    };
    let back = VfsError::from_io(limit.into_io(), "/outer");
    assert!(
        matches!(&back, VfsError::ArchiveLimit { path, limit } if path.as_path() == std::path::Path::new("/inner/entry") && *limit == "compression ratio"),
        "{back:?}"
    );
    let denied = VfsError::PermissionDenied { path: "/x".into() }.into_io();
    assert_eq!(denied.kind(), io::ErrorKind::PermissionDenied);
}

/// One error of every variant. The match stops compiling when a variant is added: give it a
/// sample here and its code in `ALL_CODES`.
fn one_of_each() -> Vec<VfsError> {
    let path = std::path::PathBuf::from("/x");
    let samples = vec![
        VfsError::NotFound { path: path.clone() },
        VfsError::PermissionDenied { path: path.clone() },
        VfsError::AlreadyExists { path: path.clone() },
        VfsError::NotADirectory { path: path.clone() },
        VfsError::IsADirectory { path: path.clone() },
        VfsError::DirectoryNotEmpty { path: path.clone() },
        VfsError::InvalidPath {
            path: path.clone(),
            reason: "test",
        },
        VfsError::Unsupported {
            path: path.clone(),
            operation: "test",
        },
        VfsError::Io {
            path: path.clone(),
            source: io::Error::other("test"),
        },
        VfsError::PasswordRequired { path: path.clone() },
        VfsError::ArchiveLimit {
            path: path.clone(),
            limit: "test",
        },
        VfsError::ArchiveDecoderMemory { path: path.clone() },
        VfsError::ArchiveDamaged {
            path: path.clone(),
            reason: "test".to_owned(),
        },
        VfsError::PasswordWrong { path: path.clone() },
        VfsError::SymlinkPrivilege { path: path.clone() },
        VfsError::TimedOut {
            path: path.clone(),
            seconds: 20,
        },
        VfsError::Cancelled { path },
    ];
    for sample in &samples {
        match sample {
            VfsError::NotFound { .. }
            | VfsError::PermissionDenied { .. }
            | VfsError::AlreadyExists { .. }
            | VfsError::NotADirectory { .. }
            | VfsError::IsADirectory { .. }
            | VfsError::DirectoryNotEmpty { .. }
            | VfsError::InvalidPath { .. }
            | VfsError::Unsupported { .. }
            | VfsError::Io { .. }
            | VfsError::PasswordRequired { .. }
            | VfsError::ArchiveLimit { .. }
            | VfsError::ArchiveDecoderMemory { .. }
            | VfsError::ArchiveDamaged { .. }
            | VfsError::PasswordWrong { .. }
            | VfsError::SymlinkPrivilege { .. }
            | VfsError::TimedOut { .. }
            | VfsError::Cancelled { .. } => {}
        }
    }
    samples
}

#[test]
fn all_codes_lists_every_code_once() {
    let codes: Vec<&str> = one_of_each().iter().map(VfsError::code).collect();
    assert_eq!(codes, VfsError::ALL_CODES);
}

/// A time limit or a cancel reported by a stream comes back as itself, not as `io`.
#[test]
fn a_timeout_and_a_cancel_keep_their_codes_through_a_stream() {
    let timed_out = VfsError::TimedOut {
        path: "/share/a".into(),
        seconds: 20,
    }
    .into_io();
    assert_eq!(timed_out.kind(), io::ErrorKind::TimedOut);
    assert_eq!(
        VfsError::from_io(timed_out, "/other").code(),
        "drive_timed_out"
    );
    let cancelled = VfsError::Cancelled { path: "/x".into() }.into_io();
    assert_eq!(cancelled.kind(), io::ErrorKind::Interrupted);
    assert_eq!(VfsError::from_io(cancelled, "/x").code(), "cancelled");
    let shown = VfsError::TimedOut {
        path: "/share".into(),
        seconds: 20,
    };
    assert_eq!(shown.to_string(), "/share: no answer within 20 s");
}
