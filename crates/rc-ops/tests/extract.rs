//! Unpacking (RC-030-03): archives of the archive backend's fixtures into a local folder - the
//! target-path guard, passwords, conflicts, cancelling, and a damaged entry that never costs the
//! file it would overwrite (RC-080-37).

mod support;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use rc_ops::{
    Answer, Backend, ConflictChoice, ErrorPolicy, ExtractSettings, JobChange, JobKind, JobOptions,
    JobSpec, JobState, PackSettings, Question,
};
use rc_vfs::{
    ArchiveFormat, ArchiveFs, ArchiveLimits, ArchivePassword, ArchiveSettings, LocalFs, VfsPath,
};
use secrecy::SecretString;
use support::{Harness, damaged_zip, payload};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../rc-vfs/tests/fixtures/archives")
        .join(name)
}

fn extract(archives: &[PathBuf], target: &Path, settings: ExtractSettings) -> JobSpec {
    JobSpec {
        kind: JobKind::Extract {
            archives: archives.iter().map(VfsPath::local).collect(),
            target: VfsPath::local(target),
            settings,
        },
        options: JobOptions::default(),
    }
}

/// Every file below `root`, relative, sorted.
fn files(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read_dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path.strip_prefix(root).expect("below root");
                found.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}

fn codes(snapshot: &rc_ops::JobSnapshot) -> Vec<&str> {
    snapshot.failures.iter().map(|f| f.code.as_str()).collect()
}

/// Acceptance: a zip-slip entry never lands outside the target; it is listed as skipped, and so
/// are absolute, drive, UNC names and the link.
#[tokio::test]
async fn unsafe_entries_are_skipped_and_listed() {
    let root = tempfile::tempdir().expect("tempdir");
    let target = root.path().join("inner").join("out");
    let harness = Harness::start();
    harness.local();
    let done = harness
        .run(extract(
            &[fixture("unsafe.zip")],
            &target,
            ExtractSettings::default(),
        ))
        .await;
    assert_eq!(done.state, JobState::Completed, "{:?}", done.failures);
    assert_eq!(files(root.path()), ["inner/out/safe.txt"]);
    assert_eq!(fs::read(target.join("safe.txt")).expect("safe"), b"safe\n");
    let codes = codes(&done);
    assert!(
        codes.iter().filter(|c| **c == "unsafe_entry").count() >= 6,
        "{codes:?}"
    );
    assert!(codes.contains(&"link_not_extracted"), "{codes:?}");
}

/// Acceptance: three archives, each into a folder of its own.
#[tokio::test]
async fn each_archive_into_its_own_folder() {
    let root = tempfile::tempdir().expect("tempdir");
    let harness = Harness::start();
    harness.local();
    let mut archives = Vec::new();
    for (fixture_name, copy) in [
        ("tree.zip", "alpha.zip"),
        ("tree.7z", "beta.7z"),
        ("tree.tar.gz", "gamma.tar.gz"),
    ] {
        let path = root.path().join(copy);
        fs::copy(fixture(fixture_name), &path).expect("copy fixture");
        archives.push(path);
    }
    let out = root.path().join("out");
    let settings = ExtractSettings {
        own_folder: true,
        ..ExtractSettings::default()
    };
    let done = harness.run(extract(&archives, &out, settings)).await;
    assert_eq!(done.state, JobState::Completed, "{:?}", done.failures);
    for folder in ["alpha", "beta", "gamma"] {
        let hello = fs::read(out.join(folder).join("hello.txt")).expect(folder);
        assert_eq!(hello, b"hello archive\n", "{folder}");
        assert!(
            out.join(folder).join("deep/nested/data.bin").is_file(),
            "{folder}"
        );
    }
    assert_eq!(done.progress.files_done, done.progress.files_total);
}

#[tokio::test]
async fn a_mask_and_flat_paths() {
    let root = tempfile::tempdir().expect("tempdir");
    let harness = Harness::start();
    harness.local();
    let settings = ExtractSettings {
        mask: "*.md".to_owned(),
        keep_paths: false,
        ..ExtractSettings::default()
    };
    let done = harness
        .run(extract(&[fixture("tree.zip")], root.path(), settings))
        .await;
    assert_eq!(done.state, JobState::Completed, "{:?}", done.failures);
    assert_eq!(files(root.path()), ["readme.md"]);
}

/// Acceptance: conflicts come as questions; "skip all" holds for the rest.
#[tokio::test]
async fn skip_all_answers_every_conflict() {
    let root = tempfile::tempdir().expect("tempdir");
    let harness = Harness::start();
    harness.local();
    let first = harness
        .run(extract(
            &[fixture("tree.zip")],
            root.path(),
            ExtractSettings::default(),
        ))
        .await;
    assert_eq!(first.state, JobState::Completed);
    let asked = Arc::new(Mutex::new(0));
    let count = Arc::clone(&asked);
    harness.on_event(move |queue, event| {
        if let JobChange::Question {
            question: Question::Conflict { .. },
        } = &event.change
        {
            *count.lock().expect("count") += 1;
            let skip = Answer::Conflict {
                choice: ConflictChoice::Skip,
                for_all: true,
            };
            let _ = queue.answer(event.job, skip);
        }
    });
    let second = harness
        .run(extract(
            &[fixture("tree.zip")],
            root.path(),
            ExtractSettings::default(),
        ))
        .await;
    assert_eq!(second.state, JobState::Completed);
    assert_eq!(*asked.lock().expect("count"), 1);
    assert_eq!(second.skipped, first.progress.files_done);
}

/// Acceptance: an encrypted 7z unpacks once the password is typed; a wrong one is told, and the
/// password shows in no event, snapshot or failure (canary).
#[tokio::test]
async fn a_password_is_asked_again_when_wrong_and_never_shown() {
    const CANARY: &str = "canary-9d41e7";
    let root = tempfile::tempdir().expect("tempdir");
    let harness = Harness::start();
    harness.local();
    let questions = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&questions);
    harness.on_event(move |queue, event| {
        if let JobChange::Question {
            question: Question::Password { wrong, .. },
        } = &event.change
        {
            seen.lock().expect("seen").push(*wrong);
            let typed = if *wrong { "secret" } else { CANARY };
            let password = ArchivePassword::new(SecretString::from(typed));
            let _ = queue.answer_password(event.job, Some(password));
        }
    });
    for name in ["encrypted.7z", "encrypted-header.7z", "encrypted-aes.zip"] {
        questions.lock().expect("questions").clear();
        let target = root.path().join(name);
        let done = harness
            .run(extract(
                &[fixture(name)],
                &target,
                ExtractSettings::default(),
            ))
            .await;
        assert_eq!(
            done.state,
            JobState::Completed,
            "{name}: {:?}",
            done.failures
        );
        assert!(done.failures.is_empty(), "{name}: {:?}", done.failures);
        assert_eq!(
            *questions.lock().expect("questions"),
            [false, true],
            "{name}"
        );
        assert!(target.join("secret.txt").is_file(), "{name}");
        let shown = format!(
            "{done:?}{}{:?}",
            serde_json::to_string(&done).expect("json"),
            harness.events()
        );
        assert!(!shown.contains(CANARY), "{name}: the password leaked");
    }
}

/// Acceptance (MCP): a job that may not ask fails encrypted entries with `password_required`.
#[tokio::test]
async fn a_job_that_may_not_ask_reports_password_required() {
    let root = tempfile::tempdir().expect("tempdir");
    let harness = Harness::start();
    harness.local();
    let mut spec = extract(
        &[fixture("encrypted.7z")],
        root.path(),
        ExtractSettings::default(),
    );
    spec.options.on_error = ErrorPolicy::Skip;
    let done = harness.run(spec).await;
    assert_eq!(done.state, JobState::Completed);
    assert_eq!(codes(&done), ["password_required"]);
    assert!(files(root.path()).is_empty());
}

/// Acceptance: a cancel in the middle of a large file leaves no half file.
#[tokio::test]
async fn a_cancel_mid_file_leaves_no_half_file() {
    let root = tempfile::tempdir().expect("tempdir");
    let src = root.path().join("big.bin");
    fs::write(&src, payload(6 << 20, 3)).expect("write");
    let archive = root.path().join("big.zip");
    let harness = Harness::start();
    harness.local();
    let packed = harness
        .run(JobSpec {
            kind: JobKind::Pack {
                sources: vec![VfsPath::local(&src)],
                target: VfsPath::local(&archive),
                settings: PackSettings {
                    level: 0,
                    ..PackSettings::default()
                },
            },
            options: JobOptions::default(),
        })
        .await;
    assert_eq!(packed.state, JobState::Completed, "{:?}", packed.failures);
    harness.on_event(|queue, event| {
        if let JobChange::Progress { progress } = &event.change
            && progress.bytes_done > 0
            && progress.bytes_done < progress.bytes_total
        {
            let _ = queue.cancel(event.job);
        }
    });
    let out = root.path().join("out");
    let done = harness
        .run(extract(&[archive], &out, ExtractSettings::default()))
        .await;
    assert_eq!(done.state, JobState::Cancelled);
    assert!(!out.join("big.bin").exists(), "a half file was left");
}

/// Acceptance (RC-080-37): overwriting a file with an entry that fails to read - its checksum is
/// wrong, which shows once its bytes are through - leaves the old file as it was, unpacked or
/// copied out of the archive: the new one is written under a part name, and the part goes.
#[tokio::test]
async fn a_damaged_entry_keeps_the_file_it_would_overwrite() {
    let root = tempfile::tempdir().expect("tempdir");
    let archive = damaged_zip(root.path());
    let out = root.path().join("out");
    fs::create_dir(&out).expect("mkdir");
    let old = out.join("hello.txt");
    let harness = Harness::start();
    harness.local();
    let inside = VfsPath::archive(VfsPath::local(&archive), ArchiveFormat::Zip, "/hello.txt");
    let reader = ArchiveFs::new(
        Arc::new(LocalFs::new()),
        &archive,
        ArchiveFormat::Zip,
        ArchiveSettings::default(),
    );
    harness
        .queue
        .register(inside.backend.clone(), Backend::new(Arc::new(reader)));
    let options = JobOptions {
        on_conflict: Some(ConflictChoice::Overwrite),
        on_error: ErrorPolicy::Skip,
        ..JobOptions::default()
    };
    let only_hello = ExtractSettings {
        mask: "hello.txt".to_owned(),
        ..ExtractSettings::default()
    };
    let mut unpack = extract(std::slice::from_ref(&archive), &out, only_hello);
    unpack.options = options;
    let copy = JobSpec {
        kind: JobKind::Copy {
            sources: vec![inside],
            target: VfsPath::local(&out),
        },
        options,
    };
    for (what, spec) in [("unpack", unpack), ("copy", copy)] {
        fs::write(&old, b"the old version\n").expect("write");
        let done = harness.run(spec).await;
        assert_eq!(
            done.state,
            JobState::Completed,
            "{what}: {:?}",
            done.failures
        );
        assert_eq!(codes(&done), ["archive_damaged"], "{what}");
        assert_eq!(fs::read(&old).expect(what), b"the old version\n", "{what}");
        assert_eq!(files(&out), ["hello.txt"], "{what}: no part is left");
    }
}

/// F6 out of an archive is refused: an archive is read-only.
#[tokio::test]
async fn moving_out_of_an_archive_is_refused() {
    let harness = Harness::start();
    harness.local();
    let inside = VfsPath::archive(
        VfsPath::local(fixture("tree.zip")),
        ArchiveFormat::Zip,
        "/hello.txt",
    );
    let refused = harness.queue.submit(JobSpec {
        kind: JobKind::Move {
            sources: vec![inside],
            target: VfsPath::local("/tmp"),
        },
        options: JobOptions::default(),
    });
    assert_eq!(
        refused.map(|_| ()).map_err(|e| e.code()),
        Err("unsupported")
    );
}

/// RC-080-35: the queue's archive limits reach an unpack job - an xz whose 8 MiB dictionary
/// passes a 1 MiB decoder memory is refused before anything is reserved or written.
#[tokio::test]
async fn an_unpack_job_reads_under_the_queues_decoder_memory() {
    let root = tempfile::tempdir().expect("tempdir");
    let harness = Harness::start();
    harness.local();
    harness.queue.set_archive_limits(ArchiveLimits {
        max_decoder_memory: 1 << 20,
        ..ArchiveLimits::default()
    });
    assert_eq!(
        harness.queue.config().archive.limits.max_decoder_memory,
        1 << 20
    );
    let mut spec = extract(
        &[fixture("single.txt.xz")],
        root.path(),
        ExtractSettings::default(),
    );
    spec.options.on_error = ErrorPolicy::Skip;
    let done = harness.run(spec).await;
    assert_eq!(
        codes(&done),
        ["archive_decoder_memory"],
        "{:?}",
        done.failures
    );
    assert!(files(root.path()).is_empty());
}
