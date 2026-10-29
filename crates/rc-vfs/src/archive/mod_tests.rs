use std::path::{Path, PathBuf};

use super::{ArchiveFormat, ArchiveFs, ArchiveSettings};
use crate::{
    BackendId, EntryKind, VfsPath,
    archive::test_support::{fixture, list, local, mtime, names, open, read},
    conformance,
};

/// The same tree in every format with more than one entry.
const TREES: &[&str] = &[
    "tree.zip",
    "tree.7z",
    "tree.tar",
    "tree.tar.gz",
    "tree.tar.xz",
    "tree.tar.bz2",
    "tree.tar.zst",
];

/// Acceptance: every format lists its fixture right - names, sizes, times, directories - and
/// reads an entry bit for bit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_format_lists_and_reads_the_same_tree() {
    let long = format!("{}.txt", "n".repeat(120));
    let data: Vec<u8> = (0..3000_u32).map(|index| (index % 251) as u8).collect();
    for name in TREES {
        let fs = open(name).await;
        let root = list(&fs, "/").await;
        assert_eq!(
            names(&root),
            [
                "conformance",
                "deep",
                "docs",
                "empty.txt",
                "hello.txt",
                "long",
                "unicode"
            ],
            "{name}"
        );
        let [_, _, docs, empty, hello, ..] = root.as_slice() else {
            panic!("{name}: seven entries");
        };
        assert_eq!(
            (&hello.kind, hello.size, hello.modified),
            (&EntryKind::File, 14, Some(mtime())),
            "{name}"
        );
        assert_eq!((&empty.kind, empty.size), (&EntryKind::File, 0), "{name}");
        assert_eq!(
            (&docs.kind, docs.size, docs.modified),
            (&EntryKind::Dir, 0, Some(mtime())),
            "{name}"
        );
        assert_eq!(names(&list(&fs, "/docs").await), ["readme.md"], "{name}");
        assert_eq!(
            names(&list(&fs, "/deep/nested").await),
            ["data.bin"],
            "{name}"
        );
        assert_eq!(
            names(&list(&fs, "/unicode").await),
            ["Gr\u{fc}\u{df}e.txt"],
            "{name}"
        );
        assert_eq!(names(&list(&fs, "/long").await), [long.as_str()], "{name}");

        assert_eq!(read(&fs, "/hello.txt").await, b"hello archive\n", "{name}");
        assert_eq!(
            read(&fs, "/docs/readme.md").await,
            b"# Readme\n\nInside an archive.\n",
            "{name}"
        );
        assert_eq!(read(&fs, "/deep/nested/data.bin").await, data, "{name}");
        assert!(read(&fs, "/empty.txt").await.is_empty(), "{name}");
        assert_eq!(
            read(&fs, "/unicode/Gr\u{fc}\u{df}e.txt").await,
            b"utf8\n",
            "{name}"
        );
        assert_eq!(
            read(&fs, &format!("/long/{long}")).await,
            b"long\n",
            "{name}"
        );

        // `deep/` has no entry of its own in the zip and the tars; 7z states it.
        let deep = fs.stat_detailed(Path::new("/deep")).await.expect("deep");
        assert_eq!(deep.entry.kind, EntryKind::Dir, "{name}");
        assert_eq!(deep.info.implied, !name.ends_with(".7z"), "{name}");
    }
}

/// The read-only part of the backend conformance suite, against each fixture's
/// `conformance/` directory, and every write refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_format_passes_the_read_only_conformance_suite() {
    for name in TREES {
        let fs = open(name).await;
        conformance::run_read_only(&fs, Path::new("/conformance")).await;
        conformance::writes_are_unsupported(&fs, Path::new("/conformance")).await;
    }
}

/// The listing is cached per archive and scanned again once the archive file changes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_listing_follows_a_changed_archive() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("archive.bin");
    std::fs::copy(fixture("cp437.zip"), &path).expect("copy");
    let fs = ArchiveFs::new(
        local(),
        &path,
        ArchiveFormat::Zip,
        ArchiveSettings::default(),
    );
    assert_eq!(names(&list(&fs, "/").await), ["Gr\u{f6}\u{df}e.txt"]);
    assert_eq!(
        names(&list(&fs, "/").await),
        ["Gr\u{f6}\u{df}e.txt"],
        "cached"
    );
    std::fs::copy(fixture("zip64.zip"), &path).expect("copy over");
    assert_eq!(names(&list(&fs, "/").await), ["zip64.txt"]);
}

#[test]
fn an_archive_path_names_its_outer_file() {
    let outer = VfsPath::local("/data/a.tar.gz");
    let format = ArchiveFormat::Tar {
        compression: Some(super::Compression::Gzip),
    };
    let inside = VfsPath::archive(outer.clone(), format, "/docs");
    assert_eq!(
        inside.backend,
        BackendId::Archive {
            outer: Box::new(outer),
            format,
        }
    );
    assert_eq!(
        inside.parent().map(|parent| parent.path),
        Some(PathBuf::from("/"))
    );
    let json = serde_json::to_string(&inside).expect("serialize");
    let back: VfsPath = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, inside);
}
