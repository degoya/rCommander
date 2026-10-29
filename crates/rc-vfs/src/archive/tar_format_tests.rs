use std::path::{Path, PathBuf};

use crate::{
    EntryKind, Vfs,
    archive::{
        UnsafeName,
        test_support::{code, list, names, open, open_path, read},
    },
};

/// Links are listed with their stored target and never followed; a hard link is shown, not
/// read; names that leave the archive are cleaned and marked as in a zip.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn links_and_escaping_names_in_a_tar() {
    let fs = open("unsafe.tar").await;
    let root = list(&fs, "/").await;
    assert_eq!(
        names(&root),
        [
            "abs-link", "abs.txt", "evil.txt", "hard", "link", "safe.txt"
        ]
    );
    let link = |target: &str| EntryKind::Symlink {
        target: PathBuf::from(target),
        target_kind: None,
    };
    assert_eq!(root[0].kind, link("/etc/shadow"));
    assert_eq!(root[4].kind, link("../../etc/passwd"));
    assert_eq!(root[3].kind, EntryKind::Other);
    for path in ["/link", "/abs-link", "/hard"] {
        assert_eq!(
            code(fs.open_read(Path::new(path)).await),
            "unsupported",
            "{path}"
        );
    }

    let evil = fs
        .stat_detailed(Path::new("/evil.txt"))
        .await
        .expect("evil");
    assert_eq!(evil.info.unsafe_name, Some(UnsafeName::ParentDir));
    let absolute = fs.stat_detailed(Path::new("/abs.txt")).await.expect("abs");
    assert_eq!(absolute.info.unsafe_name, Some(UnsafeName::Absolute));
    assert_eq!(read(&fs, "/evil.txt").await, b"evil\n");
    assert_eq!(read(&fs, "/abs.txt").await, b"abs\n");
}

/// A tar has no index: reading walks to the entry by its position, in a plain tar with seeks and
/// in a compressed one by unpacking, and both find the same bytes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_entry_is_found_again_by_its_position() {
    for name in ["tree.tar", "tree.tar.gz"] {
        let fs = open(name).await;
        assert_eq!(read(&fs, "/conformance/a.txt").await, b"hello", "{name}");
        assert_eq!(read(&fs, "/hello.txt").await, b"hello archive\n", "{name}");
        assert_eq!(read(&fs, "/conformance/b.bin").await, [0; 1024], "{name}");
    }
}

/// RC-080-12 (X6): the set-uid, set-gid and sticky bits a tar stores never reach the listing, so
/// a copy out of the archive keeps only the permission bits, as unpacking does.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_the_permission_bits_of_a_mode_are_listed() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("modes.tar");
    let mut builder = tar::Builder::new(Vec::new());
    for (name, mode) in [("plain", 0o644), ("sgid", 0o2750), ("suid", 0o4755)] {
        let mut header = tar::Header::new_gnu();
        header.set_size(1);
        header.set_mode(mode);
        header.set_entry_type(tar::EntryType::Regular);
        builder
            .append_data(&mut header, name, &b"x"[..])
            .expect("append");
    }
    std::fs::write(&path, builder.into_inner().expect("tar")).expect("write");
    let fs = open_path(path, crate::ArchiveSettings::default()).await;
    let modes: Vec<Option<u32>> = list(&fs, "/")
        .await
        .iter()
        .map(|entry| entry.attributes.mode)
        .collect();
    assert_eq!(modes, [Some(0o644), Some(0o750), Some(0o755)]);
}
