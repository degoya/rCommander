//! The boundaries of the archive backend, after rDownloader's `boundary_tests.rs`: names that
//! leave the archive, links, and the limits against bombs - each proven on a fixture an attacker
//! could have written.

use std::path::Path;

use super::{ArchiveLimits, Budget, UnsafeName, clean_name};
use crate::{
    EntryKind, Vfs, VfsError,
    archive::{
        ArchiveSettings,
        test_support::{code, list, names, open, open_path, open_with, read, zstd_window_bomb},
    },
};

#[test]
fn names_are_cleaned_and_their_trouble_noted() {
    let cases: [(&str, Vec<&str>, Option<UnsafeName>); 14] = [
        ("safe/name.txt", vec!["safe", "name.txt"], None),
        ("./dot/./x", vec!["dot", "x"], None),
        ("dir/", vec!["dir"], None),
        ("../evil.txt", vec!["evil.txt"], Some(UnsafeName::ParentDir)),
        (
            "ok/../../escape.txt",
            vec!["ok", "escape.txt"],
            Some(UnsafeName::ParentDir),
        ),
        (
            "dir\\..\\..\\x",
            vec!["dir", "x"],
            Some(UnsafeName::ParentDir),
        ),
        (
            "/abs/root.txt",
            vec!["abs", "root.txt"],
            Some(UnsafeName::Absolute),
        ),
        ("\\abs.txt", vec!["abs.txt"], Some(UnsafeName::Absolute)),
        (
            "C:/drive.txt",
            vec!["drive.txt"],
            Some(UnsafeName::DriveLetter),
        ),
        (
            "D:\\back\\slash.txt",
            vec!["back", "slash.txt"],
            Some(UnsafeName::DriveLetter),
        ),
        (
            "c:relative.txt",
            vec!["relative.txt"],
            Some(UnsafeName::DriveLetter),
        ),
        (
            "\\\\server\\share\\unc.txt",
            vec!["server", "share", "unc.txt"],
            Some(UnsafeName::Unc),
        ),
        (
            "//server/share/unc.txt",
            vec!["server", "share", "unc.txt"],
            Some(UnsafeName::Unc),
        ),
        ("..", vec![], Some(UnsafeName::ParentDir)),
    ];
    for (name, parts, issue) in cases {
        let clean = clean_name(name);
        assert_eq!(clean.parts, parts, "{name}");
        assert_eq!(clean.issue, issue, "{name}");
    }
}

/// Zip slip: every escaping name is listed under its cleaned path, marked, and read from the
/// archive by its index - the stored name is never resolved as a path.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn escaping_names_are_marked_and_never_resolved() {
    let fs = open("unsafe.zip").await;
    let root = list(&fs, "/").await;
    assert_eq!(
        names(&root),
        [
            "abs",
            "back",
            "drive.txt",
            "evil.txt",
            "link",
            "ok",
            "safe.txt",
            "server"
        ]
    );
    let cases = [
        ("/evil.txt", "../evil.txt", UnsafeName::ParentDir),
        (
            "/ok/escape.txt",
            "ok/../../escape.txt",
            UnsafeName::ParentDir,
        ),
        ("/abs/root.txt", "/abs/root.txt", UnsafeName::Absolute),
        ("/drive.txt", "C:/drive.txt", UnsafeName::DriveLetter),
        (
            "/back/slash.txt",
            "D:\\back\\slash.txt",
            UnsafeName::DriveLetter,
        ),
        (
            "/server/share/unc.txt",
            "\\\\server\\share\\unc.txt",
            UnsafeName::Unc,
        ),
    ];
    for (path, stored, issue) in cases {
        let item = fs.stat_detailed(Path::new(path)).await.expect(path);
        assert_eq!(item.info.unsafe_name, Some(issue), "{path}");
        assert_eq!(item.info.stored_name, stored, "{path}");
    }
    assert_eq!(read(&fs, "/evil.txt").await, b"evil\n");
    assert_eq!(read(&fs, "/server/share/unc.txt").await, b"unc\n");

    // `../safe.txt` cleans to the path of `safe.txt`: the safe entry keeps it.
    let safe = fs
        .stat_detailed(Path::new("/safe.txt"))
        .await
        .expect("safe");
    assert_eq!(safe.info.unsafe_name, None);
    assert_eq!(read(&fs, "/safe.txt").await, b"safe\n");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_link_is_shown_but_never_followed() {
    let fs = open("unsafe.zip").await;
    let link = fs.stat(Path::new("/link")).await.expect("stat");
    assert_eq!(
        link.kind,
        EntryKind::Symlink {
            target: "/etc/passwd".into(),
            target_kind: None,
        }
    );
    assert_eq!(code(fs.open_read(Path::new("/link")).await), "unsupported");
}

/// A bomb unpacks to far more than it stores. The zip's ratio is refused before a byte is
/// unpacked; a compressed tar has no per-entry sizes, so the guard stops its stream.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_archive_bomb_fails_instead_of_filling_memory_or_disk() {
    let strict = ArchiveSettings {
        limits: ArchiveLimits {
            max_ratio: 100,
            ratio_threshold: 1 << 20,
            ..ArchiveLimits::default()
        },
        ..ArchiveSettings::default()
    };
    let zip = open_with("bomb.zip", strict.clone()).await;
    let entries = list(&zip, "/").await;
    assert_eq!(
        entries[0].size,
        16 << 20,
        "the listing still shows the bomb"
    );
    let refused = zip.open_read(Path::new("/zeros.bin")).await;
    assert!(
        matches!(
            &refused,
            Err(VfsError::ArchiveLimit {
                limit: "compression ratio",
                ..
            })
        ),
        "{:?}",
        refused.err()
    );

    let tar = open_with("bomb.tar.gz", strict).await;
    assert_eq!(code(tar.list(Path::new("/")).await), "archive_limit");

    // With the defaults both are ordinary archives of 16 MiB.
    assert_eq!(
        read(&open("bomb.zip").await, "/zeros.bin").await.len(),
        16 << 20
    );
    assert_eq!(
        list(&open("bomb.tar.gz").await, "/").await[0].size,
        16 << 20
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn too_many_entries_or_bytes_refuse_the_listing() {
    let with = |limits| ArchiveSettings {
        limits,
        ..ArchiveSettings::default()
    };
    let few = with(ArchiveLimits {
        max_entries: 3,
        ..ArchiveLimits::default()
    });
    let small = with(ArchiveLimits {
        max_unpacked_size: 1 << 20,
        ..ArchiveLimits::default()
    });
    for (name, settings, limit) in [
        ("tree.zip", few.clone(), "entry count"),
        ("tree.7z", few.clone(), "entry count"),
        ("tree.tar", few, "entry count"),
        ("bomb.zip", small.clone(), "unpacked size"),
        ("bomb.tar.gz", small, "unpacked size"),
    ] {
        let fs = open_with(name, settings).await;
        let refused = fs.list(Path::new("/")).await;
        assert!(
            matches!(&refused, Err(VfsError::ArchiveLimit { limit: got, .. }) if *got == limit),
            "{name}: {:?}",
            refused.err()
        );
    }
}

#[test]
fn the_budget_counts_entries_and_stated_bytes() {
    let limits = ArchiveLimits {
        max_entries: 2,
        max_unpacked_size: 100,
        ..ArchiveLimits::default()
    };
    let path = Path::new("a.zip");
    let mut budget = Budget::new(&limits, path);
    budget.admit(60).expect("one");
    budget.admit(40).expect("two, exactly at both limits");
    assert!(matches!(
        budget.admit(0),
        Err(VfsError::ArchiveLimit {
            limit: "entry count",
            ..
        })
    ));
    assert!(matches!(
        Budget::new(&limits, path).admit(101),
        Err(VfsError::ArchiveLimit {
            limit: "unpacked size",
            ..
        })
    ));
}

#[test]
fn the_ratio_counts_only_above_its_threshold() {
    let limits = ArchiveLimits {
        max_ratio: 10,
        ratio_threshold: 1000,
        ..ArchiveLimits::default()
    };
    let path = Path::new("a.zip");
    assert!(
        limits.check_entry(999, Some(1), path).is_ok(),
        "below the threshold"
    );
    assert!(
        limits.check_entry(1000, Some(100), path).is_ok(),
        "exactly at the ratio"
    );
    assert!(
        limits.check_entry(1001, Some(100), path).is_err(),
        "past the ratio"
    );
    assert!(
        limits.check_entry(1 << 30, None, path).is_ok(),
        "no packed size known"
    );
    assert!(limits.check_stream(5000, 400, path).is_err());
    assert!(limits.check_stream(5000, 500, path).is_ok());
}

/// The defaults pass what an ordinary archive is and stop what a bomb is.
#[test]
fn the_defaults_are_safe_but_not_in_the_way() {
    let limits = ArchiveLimits::default();
    let path = Path::new("a.zip");
    // A 4 GiB entry of text at 20:1.
    assert!(
        limits
            .check_entry(4 << 30, Some((4 << 30) / 20), path)
            .is_ok()
    );
    // 42.zip's innermost entries: 4.3 GB each at deflate's best, about 1000:1.
    assert!(
        limits
            .check_entry(4_294_967_295, Some(4_168_000), path)
            .is_err()
    );
}

/// RC-080-13: a `.zst` whose frame asks for a 2 GiB window - opened, listed and read as a panel
/// or an agent's `archives_list` would - fails with an error, at once and in bounded memory.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_zstd_window_bomb_fails_instead_of_filling_memory() {
    use tokio::io::AsyncReadExt as _;

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("bomb.txt.zst");
    std::fs::write(&path, zstd_window_bomb()).expect("write");
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let fs = open_path(path, ArchiveSettings::default()).await;
        let entries = match fs.list(Path::new("/")).await {
            Ok(entries) => entries,
            Err(error) => return error.code(),
        };
        for entry in entries {
            let mut stream = match fs.open_read(&Path::new("/").join(&entry.name)).await {
                Ok(stream) => stream,
                Err(error) => return error.code(),
            };
            let mut sink = Vec::new();
            if stream.read_to_end(&mut sink).await.is_err() {
                return "read_failed";
            }
        }
        "ok"
    })
    .await
    .expect("never hangs");
    assert_ne!(outcome, "ok", "the bomb is refused");
}
