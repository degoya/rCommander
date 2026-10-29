use std::{
    path::Path,
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, SystemTime},
};

use async_trait::async_trait;
use rc_vfs::{Attributes, Entry, EntryKind, MemoryFs, Vfs, WriteMode, conformance};

use super::{
    Answer, ConflictChoice, Decision, ErrorChoice, Question, RENAME_CANDIDATES, auto_rename,
    decide, numbered,
};
use crate::{
    Failure,
    control::Control,
    runner::{Step, Stop},
};

fn file(modified: Option<u64>) -> Entry {
    Entry {
        name: "f".to_owned(),
        kind: EntryKind::File,
        size: 1,
        modified: modified.map(|secs| SystemTime::UNIX_EPOCH + Duration::from_secs(secs)),
        created: None,
        attributes: Attributes::default(),
        hidden: false,
        owner: None,
    }
}

#[test]
fn every_choice_decides() {
    let (source, target) = (file(Some(20)), file(Some(10)));
    assert_eq!(
        decide(ConflictChoice::Overwrite, &source, &target),
        Decision::Overwrite
    );
    assert_eq!(
        decide(ConflictChoice::Skip, &source, &target),
        Decision::Skip
    );
    assert_eq!(
        decide(ConflictChoice::Rename, &source, &target),
        Decision::Rename
    );
    assert_eq!(
        decide(ConflictChoice::Cancel, &source, &target),
        Decision::Cancel
    );
}

#[test]
fn overwrite_older_replaces_only_an_older_target() {
    let older = file(Some(10));
    let newer = file(Some(20));
    assert_eq!(
        decide(ConflictChoice::OverwriteOlder, &newer, &older),
        Decision::Overwrite
    );
    assert_eq!(
        decide(ConflictChoice::OverwriteOlder, &older, &newer),
        Decision::Skip
    );
    assert_eq!(
        decide(ConflictChoice::OverwriteOlder, &older, &older),
        Decision::Skip,
        "the same time is not older"
    );
    assert_eq!(
        decide(ConflictChoice::OverwriteOlder, &newer, &file(None)),
        Decision::Skip,
        "an unknown time is not older"
    );
}

#[test]
fn numbered_names_keep_the_extension() {
    assert_eq!(numbered("report.pdf", 2), "report (2).pdf");
    assert_eq!(numbered("archive.tar.gz", 3), "archive.tar (3).gz");
    assert_eq!(numbered(".profile", 2), ".profile (2)");
    assert_eq!(numbered("README", 2), "README (2)");
}

#[tokio::test]
async fn auto_rename_takes_the_first_free_number() {
    let fs = MemoryFs::new();
    for name in ["/a.txt", "/a (2).txt"] {
        conformance::write_file(&fs, Path::new(name), WriteMode::CreateNew, b"x").await;
    }
    let free = auto_rename(&fs as &dyn Vfs, Path::new("/a.txt"), &Control::default())
        .await
        .expect("a free name");
    assert_eq!(free, Path::new("/a (3).txt"));
}

/// A folder where every name is taken: each `stat` finds a file, and is counted.
struct Full {
    fs: MemoryFs,
    stats: AtomicU32,
}

#[async_trait]
impl rc_vfs::testing::Forward for Full {
    fn inner(&self) -> &dyn Vfs {
        &self.fs
    }
    async fn stat(&self, _path: &Path) -> rc_vfs::Result<Entry> {
        self.stats.fetch_add(1, Ordering::Relaxed);
        Ok(file(None))
    }
}

#[tokio::test]
async fn auto_rename_gives_up_after_its_candidates() {
    let full = Full {
        fs: MemoryFs::new(),
        stats: AtomicU32::new(0),
    };
    let found = auto_rename(&full as &dyn Vfs, Path::new("/a.txt"), &Control::default()).await;
    assert!(
        matches!(&found, Err(Step::Fail(error)) if error.code() == "already_exists"),
        "{found:?}"
    );
    assert_eq!(full.stats.load(Ordering::Relaxed), RENAME_CANDIDATES);
}

#[tokio::test]
async fn auto_rename_stops_when_the_job_is_cancelled() {
    let full = Full {
        fs: MemoryFs::new(),
        stats: AtomicU32::new(0),
    };
    let control = Control::default();
    control.cancel();
    let found = auto_rename(&full as &dyn Vfs, Path::new("/a.txt"), &control).await;
    assert!(
        matches!(found, Err(Step::Stop(Stop::Cancelled))),
        "{found:?}"
    );
    assert_eq!(full.stats.load(Ordering::Relaxed), 0);
}

#[test]
fn an_answer_fits_its_question_only() {
    let failure = Failure {
        path: rc_vfs::VfsPath::local("/x"),
        code: "io".to_owned(),
        message: "m".to_owned(),
        target: None,
    };
    let error = Question::Error(failure);
    let conflict = Answer::Conflict {
        choice: ConflictChoice::Skip,
        for_all: true,
    };
    let retry = Answer::Error {
        choice: ErrorChoice::Retry,
    };
    assert!(retry.fits(&error));
    assert!(!conflict.fits(&error));
}

#[test]
fn answers_read_from_json() {
    let answer: Answer =
        serde_json::from_str(r#"{"type":"conflict","choice":"overwrite_older"}"#).expect("json");
    assert_eq!(
        answer,
        Answer::Conflict {
            choice: ConflictChoice::OverwriteOlder,
            for_all: false
        }
    );
}

fn sized(size: u64) -> Entry {
    Entry {
        size,
        ..file(Some(1))
    }
}

#[test]
fn larger_and_smaller_compare_sizes_and_rename_existing_decides_alone() {
    let (big, small) = (sized(10), sized(5));
    assert_eq!(
        decide(ConflictChoice::OverwriteLarger, &big, &small),
        Decision::Overwrite
    );
    assert_eq!(
        decide(ConflictChoice::OverwriteLarger, &small, &big),
        Decision::Skip
    );
    assert_eq!(
        decide(ConflictChoice::OverwriteLarger, &small, &small),
        Decision::Skip,
        "the same size is not larger"
    );
    assert_eq!(
        decide(ConflictChoice::OverwriteSmaller, &small, &big),
        Decision::Overwrite
    );
    assert_eq!(
        decide(ConflictChoice::OverwriteSmaller, &big, &small),
        Decision::Skip
    );
    assert_eq!(
        decide(ConflictChoice::RenameExisting, &big, &small),
        Decision::RenameExisting
    );
    // Resume continues a resumable part only (`Runner::conflict`): any other target stays.
    assert_eq!(decide(ConflictChoice::Resume, &big, &small), Decision::Skip);
}

#[test]
fn a_typed_name_and_compare_fit_a_conflict_only() {
    let conflict = Question::Conflict {
        source: super::EntryInfo::new(&rc_vfs::VfsPath::local("/a"), &file(None)),
        target: super::EntryInfo::new(&rc_vfs::VfsPath::local("/b"), &file(None)),
        resumable: false,
    };
    let typed = Answer::RenameTo {
        name: "x.txt".to_owned(),
    };
    assert!(typed.fits(&conflict));
    assert!(Answer::Compare.fits(&conflict));
    let error = Question::Error(Failure {
        path: rc_vfs::VfsPath::local("/x"),
        code: "io".to_owned(),
        message: "m".to_owned(),
        target: None,
    });
    assert!(!typed.fits(&error));
    assert!(!Answer::Compare.fits(&error));
}
