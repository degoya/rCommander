use std::io;

use rc_vfs::VfsError;

use super::OpsError;

/// One error of every own variant. The match stops compiling when a variant is added: give it a
/// sample here and its code in `ALL_CODES`.
fn one_of_each_own() -> Vec<OpsError> {
    let path = || std::path::PathBuf::from("/x");
    let samples = vec![
        OpsError::JobNotFound { job: 1 },
        OpsError::JobFinished { job: 1 },
        OpsError::NoQuestion { job: 1 },
        OpsError::JobNotWaiting { job: 1 },
        OpsError::BackendUnavailable,
        OpsError::NoSources,
        OpsError::MixedSources,
        OpsError::TargetInsideSource { path: path() },
        OpsError::SameLocation { path: path() },
        OpsError::TrashUnavailable { path: path() },
        OpsError::TrashFailed {
            path: path(),
            reason: "r".to_owned(),
        },
        OpsError::DiskFull { path: path() },
        OpsError::VerifyFailed { path: path() },
        OpsError::KindMismatch { path: path() },
        OpsError::InvalidName {
            name: "..".to_owned(),
            reason: "r",
        },
        OpsError::NotCopyable { path: path() },
        OpsError::JournalFailed {
            reason: "r".to_owned(),
        },
        OpsError::NotAnArchive { path: path() },
        OpsError::UnsafeEntry {
            path: path(),
            reason: "r",
        },
        OpsError::LinkNotExtracted { path: path() },
        OpsError::PackFailed { path: path() },
        OpsError::QueueListNotFound { list: 3 },
        OpsError::QueueListFixed,
        OpsError::QueueNameInvalid { reason: "empty" },
        OpsError::JobAtOnce { job: 1 },
        OpsError::RenameUndoFailed { path: path() },
        OpsError::Sensitive {
            path: path(),
            reason: "a key",
        },
    ];
    for sample in &samples {
        match sample {
            OpsError::Vfs(_)
            | OpsError::JobNotFound { .. }
            | OpsError::JobFinished { .. }
            | OpsError::NoQuestion { .. }
            | OpsError::JobNotWaiting { .. }
            | OpsError::BackendUnavailable
            | OpsError::NoSources
            | OpsError::MixedSources
            | OpsError::TargetInsideSource { .. }
            | OpsError::SameLocation { .. }
            | OpsError::TrashUnavailable { .. }
            | OpsError::TrashFailed { .. }
            | OpsError::DiskFull { .. }
            | OpsError::VerifyFailed { .. }
            | OpsError::KindMismatch { .. }
            | OpsError::InvalidName { .. }
            | OpsError::NotCopyable { .. }
            | OpsError::JournalFailed { .. }
            | OpsError::NotAnArchive { .. }
            | OpsError::UnsafeEntry { .. }
            | OpsError::LinkNotExtracted { .. }
            | OpsError::PackFailed { .. }
            | OpsError::QueueListNotFound { .. }
            | OpsError::QueueListFixed
            | OpsError::QueueNameInvalid { .. }
            | OpsError::JobAtOnce { .. }
            | OpsError::RenameUndoFailed { .. }
            | OpsError::Sensitive { .. } => {}
        }
    }
    samples
}

#[test]
fn all_codes_lists_every_own_code_once() {
    let codes: Vec<&str> = one_of_each_own().iter().map(OpsError::code).collect();
    assert_eq!(codes, OpsError::ALL_CODES);
}

#[test]
fn a_file_system_failure_keeps_its_code() {
    let error = OpsError::from(VfsError::PermissionDenied { path: "/x".into() });
    assert_eq!(error.code(), "permission_denied");
    assert!(!OpsError::ALL_CODES.contains(&error.code()));
}

#[test]
fn a_full_disk_is_its_own_code() {
    let full = OpsError::from_io(io::Error::from(io::ErrorKind::StorageFull), "/t");
    assert_eq!(full.code(), "disk_full");
    let quota = OpsError::from_io(io::Error::from(io::ErrorKind::QuotaExceeded), "/t");
    assert_eq!(quota.code(), "disk_full");
    let other = OpsError::from_io(io::Error::other("boom"), "/t");
    assert_eq!(other.code(), "io");
}
