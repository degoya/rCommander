use super::*;
use crate::{Layout, create, test_support};

const SHA256_ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn statuses(report: &VerifyReport) -> Vec<(String, VerifyStatus, Option<String>)> {
    report
        .entries
        .iter()
        .map(|entry| (entry.name.clone(), entry.status, entry.code.clone()))
        .collect()
}

#[tokio::test]
async fn every_line_gets_ok_mismatch_missing_unreadable_or_invalid() {
    let outer = tempfile::tempdir().expect("outer");
    test_support::write(outer.path(), "secret", b"abc");
    let dir = outer.path().join("set");
    test_support::write(&dir, "good.txt", b"abc");
    test_support::write(&dir, "changed.txt", b"abd");
    std::fs::create_dir(dir.join("folder")).expect("dir");
    let text = format!(
        "{SHA256_ABC} *good.txt\n\
         {SHA256_ABC} *changed.txt\n\
         {SHA256_ABC} *gone.txt\n\
         {SHA256_ABC} *folder\n\
         {SHA256_ABC} *../secret\n\
         {SHA256_ABC} */etc/passwd\n\
         this is no checksum line\n"
    );
    test_support::write(&dir, "set.sha256", text.as_bytes());
    let vfs = test_support::local().1;
    let control = Control::new();
    let report = verify(&vfs, &dir.join("set.sha256"), &control)
        .await
        .expect("verified");
    // A check counts its lines, not bytes it cannot know before it hashes.
    assert_eq!((control.files_done(), control.files_total()), (7, 7));
    assert_eq!(
        statuses(&report),
        vec![
            ("good.txt".to_owned(), VerifyStatus::Ok, None),
            ("changed.txt".to_owned(), VerifyStatus::Mismatch, None),
            ("gone.txt".to_owned(), VerifyStatus::Missing, None),
            (
                "folder".to_owned(),
                VerifyStatus::Unreadable,
                Some("is_a_directory".to_owned())
            ),
            (
                "../secret".to_owned(),
                VerifyStatus::Invalid,
                Some("parent_reference".to_owned())
            ),
            (
                "/etc/passwd".to_owned(),
                VerifyStatus::Invalid,
                Some("absolute_name".to_owned())
            ),
            (
                String::new(),
                VerifyStatus::Invalid,
                Some("unknown_format".to_owned())
            ),
        ]
    );
    assert_eq!(
        report.summary,
        VerifySummary {
            ok: 1,
            mismatch: 1,
            missing: 1,
            unreadable: 1,
            invalid: 3,
        }
    );
    assert!(!report.summary.all_ok());
    let changed = &report.entries[1];
    assert_eq!(changed.expected.as_deref(), Some(SHA256_ABC));
    assert_ne!(changed.actual.as_deref(), Some(SHA256_ABC));
    assert_eq!(report.entries[6].line, 7);
}

#[tokio::test]
async fn what_create_writes_verifies_and_a_tampered_file_is_caught() {
    for algorithm in Algorithm::ALL {
        let (root, vfs) = test_support::local();
        test_support::write(root.path(), "a.bin", &test_support::pattern(70_000));
        test_support::write(root.path(), "sub/b c.bin", b"second");
        let names = vec!["a.bin".to_owned(), "sub".to_owned()];
        let report = create(
            &vfs,
            root.path(),
            &names,
            algorithm,
            &Layout::Combined { file_name: None },
            &Control::new(),
        )
        .await
        .expect("created");
        let sums = root.path().join(&report.written[0]);
        let clean = verify(&vfs, &sums, &Control::new())
            .await
            .expect("verified");
        assert!(clean.summary.all_ok(), "{algorithm:?}: {clean:?}");
        assert_eq!(clean.summary.ok, 2);
        test_support::write(root.path(), "sub/b c.bin", b"Second");
        let tampered = verify(&vfs, &sums, &Control::new())
            .await
            .expect("verified");
        assert_eq!(tampered.summary.mismatch, 1, "{algorithm:?}");
        assert_eq!(tampered.entries[1].name, "sub/b c.bin");
    }
}

#[tokio::test]
async fn sfv_bsd_and_windows_separators_are_read() {
    let (root, vfs) = test_support::local();
    test_support::write(root.path(), "sub/a.txt", b"abc");
    test_support::write(
        root.path(),
        "list.sfv",
        b"; from another tool\r\nsub\\a.txt 352441C2\r\n",
    );
    test_support::write(
        root.path(),
        "tags.txt",
        format!("SHA256 (sub/a.txt) = {SHA256_ABC}\n").as_bytes(),
    );
    for name in ["list.sfv", "tags.txt"] {
        let report = verify(&vfs, &root.path().join(name), &Control::new())
            .await
            .expect("verified");
        assert!(report.summary.all_ok(), "{name}: {report:?}");
        assert_eq!(report.summary.ok, 1, "{name}");
    }
}

#[tokio::test]
async fn files_that_are_no_checksum_files_or_too_large_are_refused() {
    let (root, vfs) = test_support::local();
    test_support::write(root.path(), "notes.md5", b"just some notes\n\n");
    let error = verify(&vfs, &root.path().join("notes.md5"), &Control::new())
        .await
        .expect_err("unknown");
    assert_eq!(error.code(), "checksum_format_unknown");
    test_support::write(root.path(), "big.sha256", &[b'a'; 100]);
    let error = read_capped(&vfs, &root.path().join("big.sha256"), 99)
        .await
        .expect_err("too large");
    assert_eq!(error.code(), "checksum_file_too_large");
    let text = read_capped(&vfs, &root.path().join("big.sha256"), 100)
        .await
        .expect("at the limit");
    assert_eq!(text.len(), 100);
}

#[tokio::test]
async fn a_cancelled_verify_stops_with_cancelled() {
    let (root, vfs) = test_support::local();
    test_support::write(root.path(), "a.txt", b"abc");
    test_support::write(
        root.path(),
        "a.sha256",
        format!("{SHA256_ABC} *a.txt\n").as_bytes(),
    );
    let control = Control::new();
    control.cancel();
    let error = verify(&vfs, &root.path().join("a.sha256"), &control)
        .await
        .expect_err("cancelled");
    assert_eq!(error.code(), "cancelled");
}

/// A guard that refuses every file named `id_rsa`.
fn no_keys() -> Control {
    Control::new().with_guard(std::sync::Arc::new(|path: &std::path::Path| {
        (path.file_name()? == "id_rsa").then_some("a test key")
    }))
}

/// A line naming a file the run's guard refuses is unreadable with `sensitive_path` and never
/// carries the file's actual digest; the other lines are checked as usual.
#[tokio::test]
async fn a_guarded_file_is_never_hashed_and_its_digest_never_reported() {
    let (root, vfs) = test_support::local();
    test_support::write(root.path(), "id_rsa", b"abc");
    test_support::write(root.path(), "a.txt", b"abc");
    test_support::write(
        root.path(),
        "set.sha256",
        format!("{SHA256_ABC} *id_rsa\n{SHA256_ABC} *a.txt\n").as_bytes(),
    );
    let report = verify(&vfs, &root.path().join("set.sha256"), &no_keys())
        .await
        .expect("verified");
    assert_eq!(
        statuses(&report),
        vec![
            (
                "id_rsa".to_owned(),
                VerifyStatus::Unreadable,
                Some("sensitive_path".to_owned())
            ),
            ("a.txt".to_owned(), VerifyStatus::Ok, None),
        ]
    );
    assert_eq!(report.entries[0].actual, None);
}

/// The progress window's File bar (RC-010-07): a check names the file of the line at work, its
/// size as found and its bytes so far - the total of a check counts lines, not bytes.
#[tokio::test]
async fn a_check_names_the_file_at_work_and_its_bytes() {
    let (root, _) = test_support::local();
    let big = 2 * crate::BUFFER_SIZE as u64;
    test_support::write(
        root.path(),
        "big.bin",
        &test_support::pattern(2 * crate::BUFFER_SIZE),
    );
    test_support::write(
        root.path(),
        "big.bin.sha256",
        format!("{SHA256_ABC} *big.bin\n").as_bytes(),
    );
    let control = Control::new();
    let vfs: Arc<dyn Vfs> = Arc::new(test_support::Tripwire {
        inner: rc_vfs::LocalFs::new(),
        control: control.clone(),
        after: crate::BUFFER_SIZE as u64,
    });
    let error = verify(&vfs, &root.path().join("big.bin.sha256"), &control)
        .await
        .expect_err("cancelled half way");
    assert_eq!(error.code(), "cancelled");
    assert_eq!(control.current_file(), Some(root.path().join("big.bin")));
    assert_eq!(control.file_bytes_total(), big);
    let done = control.file_bytes_done();
    assert!(
        (crate::BUFFER_SIZE as u64..big).contains(&done),
        "half way: {done}"
    );
}
