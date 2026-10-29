use std::path::PathBuf;

use rc_vfs::VfsError;

use super::ChecksumError;

/// One error of every own variant. The match stops compiling when a variant is added: give it a
/// sample here and its code in `ALL_CODES`.
fn one_of_each_own() -> Vec<ChecksumError> {
    let path = PathBuf::from("/x");
    let samples = vec![
        ChecksumError::Cancelled,
        ChecksumError::InvalidName {
            name: "a/b".to_owned(),
        },
        ChecksumError::NoFiles,
        ChecksumError::NotAFile { path: path.clone() },
        ChecksumError::ChecksumFileTooLarge {
            path: path.clone(),
            size: 1,
        },
        ChecksumError::UnknownFormat { path: path.clone() },
        ChecksumError::InvalidCrcFile {
            path: path.clone(),
            reason: "x",
        },
        ChecksumError::InvalidPartSize,
        ChecksumError::TooManyParts {
            parts: 1000,
            max: 999,
        },
        ChecksumError::NotASplitFile { path },
        ChecksumError::PartMissing {
            name: "a.002".to_owned(),
        },
        ChecksumError::SizeMismatch {
            expected: 2,
            actual: 1,
        },
        ChecksumError::CrcMismatch {
            expected: "00000000".to_owned(),
            actual: "00000001".to_owned(),
        },
        ChecksumError::Sensitive {
            path: PathBuf::from("/x"),
            reason: "x",
        },
    ];
    for sample in &samples {
        match sample {
            ChecksumError::Vfs(_)
            | ChecksumError::Cancelled
            | ChecksumError::InvalidName { .. }
            | ChecksumError::NoFiles
            | ChecksumError::NotAFile { .. }
            | ChecksumError::ChecksumFileTooLarge { .. }
            | ChecksumError::UnknownFormat { .. }
            | ChecksumError::InvalidCrcFile { .. }
            | ChecksumError::InvalidPartSize
            | ChecksumError::TooManyParts { .. }
            | ChecksumError::NotASplitFile { .. }
            | ChecksumError::PartMissing { .. }
            | ChecksumError::SizeMismatch { .. }
            | ChecksumError::CrcMismatch { .. }
            | ChecksumError::Sensitive { .. } => {}
        }
    }
    samples
}

#[test]
fn every_own_code_is_listed_once() {
    let codes: Vec<&str> = one_of_each_own().iter().map(ChecksumError::code).collect();
    assert_eq!(codes, ChecksumError::ALL_CODES);
}

#[test]
fn a_file_system_failure_keeps_its_code() {
    let error = ChecksumError::from(VfsError::NotFound {
        path: PathBuf::from("/x"),
    });
    assert_eq!(error.code(), "not_found");
    assert!(!ChecksumError::ALL_CODES.contains(&error.code()));
}
