use rc_vfs::LocalFs;

use super::*;
use crate::{
    combine,
    test_support::{self, Tripwire, names},
};

async fn split_here(
    vfs: &Arc<dyn Vfs>,
    dir: &Path,
    name: &str,
    part_size: u64,
) -> Result<SplitReport, ChecksumError> {
    split(
        vfs,
        &dir.join(name),
        vfs,
        &dir.join("out"),
        part_size,
        &Control::new(),
    )
    .await
}

#[tokio::test]
async fn parts_take_the_stem_and_the_crc_file_names_the_original() {
    let (root, vfs) = test_support::local();
    let data = test_support::pattern(10_000);
    test_support::write(root.path(), "disk.iso", &data);
    std::fs::create_dir(root.path().join("out")).expect("out");
    let report = split_here(&vfs, root.path(), "disk.iso", 3_000)
        .await
        .expect("split");
    assert_eq!(
        report.parts,
        vec!["disk.001", "disk.002", "disk.003", "disk.004"]
    );
    assert_eq!(report.crc_file, "disk.crc");
    assert_eq!(report.size, 10_000);
    let out = root.path().join("out");
    let sizes: Vec<u64> = report
        .parts
        .iter()
        .map(|part| std::fs::metadata(out.join(part)).expect("part").len())
        .collect();
    assert_eq!(sizes, vec![3_000, 3_000, 3_000, 1_000]);
    let mut crc = Algorithm::Crc32.hasher();
    crc.update(&data);
    let crc = crc.finish_hex();
    assert_eq!(report.crc32, crc);
    assert_eq!(
        std::fs::read_to_string(out.join("disk.crc")).expect("crc"),
        format!(
            "filename=disk.iso\r\nsize=10000\r\ncrc32={}\r\n",
            crc.to_uppercase()
        )
    );
}

#[tokio::test]
async fn split_and_combine_give_back_the_same_bytes() {
    // The same code path as 1 GB parts of a 5 GB file, at a size a test can afford: several
    // buffers per part and a part boundary inside a buffer.
    let (root, vfs) = test_support::local();
    let data = test_support::pattern(3 * BUFFER_SIZE + 12_345);
    test_support::write(root.path(), "big.bin", &data);
    std::fs::create_dir(root.path().join("out")).expect("out");
    std::fs::create_dir(root.path().join("back")).expect("back");
    let part_size = BUFFER_SIZE as u64 + 777;
    let report = split_here(&vfs, root.path(), "big.bin", part_size)
        .await
        .expect("split");
    assert_eq!(report.parts.len(), 4);
    let out = root.path().join("out");
    let back = root.path().join("back");
    for start in ["big.001", "big.crc", "big.003"] {
        let combined = combine(&vfs, &out.join(start), &vfs, &back, None, &Control::new())
            .await
            .expect("combined");
        assert_eq!(combined.target, "big.bin");
        assert!(combined.crc_checked);
        assert_eq!(combined.crc32, report.crc32);
        assert_eq!(combined.size, data.len() as u64);
        assert_eq!(std::fs::read(back.join("big.bin")).expect("back"), data);
        std::fs::remove_file(back.join("big.bin")).expect("clean");
    }
}

/// A job's progress: split and combine say how many parts and bytes they handle, and both
/// end at their totals.
#[tokio::test]
async fn split_and_combine_count_their_parts_and_bytes() {
    let (root, vfs) = test_support::local();
    test_support::write(root.path(), "disk.iso", &test_support::pattern(10_000));
    std::fs::create_dir(root.path().join("out")).expect("out");
    let out = root.path().join("out");
    let control = Control::new();
    split(
        &vfs,
        &root.path().join("disk.iso"),
        &vfs,
        &out,
        3_000,
        &control,
    )
    .await
    .expect("split");
    let counts = |control: &Control| {
        (
            control.files_done(),
            control.files_total(),
            control.bytes_done(),
            control.bytes_total(),
        )
    };
    assert_eq!(counts(&control), (4, 4, 10_000, 10_000));
    let control = Control::new();
    combine(
        &vfs,
        &out.join("disk.001"),
        &vfs,
        root.path(),
        Some("back.iso"),
        &control,
    )
    .await
    .expect("combined");
    assert_eq!(counts(&control), (4, 4, 10_000, 10_000));
}

#[tokio::test]
async fn an_exact_multiple_ends_without_an_empty_part_and_an_empty_file_has_one() {
    let (root, vfs) = test_support::local();
    test_support::write(root.path(), "even.bin", &test_support::pattern(6_000));
    test_support::write(root.path(), "empty", b"");
    std::fs::create_dir(root.path().join("out")).expect("out");
    let even = split_here(&vfs, root.path(), "even.bin", 3_000)
        .await
        .expect("split");
    assert_eq!(even.parts, vec!["even.001", "even.002"]);
    let empty = split_here(&vfs, root.path(), "empty", 3_000)
        .await
        .expect("split");
    assert_eq!(empty.parts, vec!["empty.001"]);
    assert_eq!(empty.crc32, "00000000");
}

#[tokio::test]
async fn refusals_write_nothing() {
    let (root, vfs) = test_support::local();
    test_support::write(root.path(), "a.bin", &test_support::pattern(1_000));
    test_support::write(root.path(), "out/a.003", b"someone else's");
    let code = |result: Result<SplitReport, ChecksumError>| result.expect_err("refused").code();
    assert_eq!(
        code(split_here(&vfs, root.path(), "a.bin", 0).await),
        "split_part_size_invalid"
    );
    assert_eq!(
        code(split_here(&vfs, root.path(), "a.bin", 1).await),
        "split_too_many_parts"
    );
    assert_eq!(
        code(split_here(&vfs, root.path(), "a.bin", 100).await),
        "already_exists"
    );
    assert_eq!(
        code(split_here(&vfs, root.path(), "out", 100).await),
        "is_a_directory"
    );
    assert_eq!(names(&root.path().join("out")), vec!["a.003"]);
}

#[tokio::test]
async fn a_cancel_half_way_removes_every_part_written() {
    let root = tempfile::tempdir().expect("root");
    test_support::write(
        root.path(),
        "a.bin",
        &test_support::pattern(4 * BUFFER_SIZE),
    );
    std::fs::create_dir(root.path().join("out")).expect("out");
    let control = Control::new();
    let vfs: Arc<dyn Vfs> = Arc::new(Tripwire {
        inner: LocalFs::new(),
        control: control.clone(),
        after: 2 * BUFFER_SIZE as u64,
    });
    let error = split(
        &vfs,
        &root.path().join("a.bin"),
        &vfs,
        &root.path().join("out"),
        BUFFER_SIZE as u64,
        &control,
    )
    .await
    .expect_err("cancelled");
    assert_eq!(error.code(), "cancelled");
    assert!(names(&root.path().join("out")).is_empty());
    // The File bar (RC-010-07): the source is the file at work, caught half way.
    let size = 4 * BUFFER_SIZE as u64;
    assert_eq!(control.current_file(), Some(root.path().join("a.bin")));
    assert_eq!(control.file_bytes_total(), size);
    let done = control.file_bytes_done();
    assert!((size / 2..size).contains(&done), "half way: {done}");
}

#[test]
fn stems_and_part_names_follow_total_commander() {
    assert_eq!(stem_of("disk.iso"), "disk");
    assert_eq!(stem_of("archive.tar.gz"), "archive.tar");
    assert_eq!(stem_of("noext"), "noext");
    assert_eq!(stem_of(".bashrc"), ".bashrc");
    assert_eq!(part_name("disk", 7), "disk.007");
    assert_eq!(part_name("disk", 999), "disk.999");
}

/// RC-080-43 (S9): a source whose name breaks the one name rule - here an NTFS stream, which a
/// Linux folder or a server may list - is not split into parts of that name.
#[cfg(unix)]
#[tokio::test]
async fn a_source_named_against_the_name_rule_is_not_split() {
    let (root, vfs) = test_support::local();
    test_support::write(root.path(), "a:b.iso", &test_support::pattern(100));
    std::fs::create_dir(root.path().join("out")).expect("out");
    let error = split_here(&vfs, root.path(), "a:b.iso", 30)
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "invalid_name");
    assert_eq!(
        std::fs::read_dir(root.path().join("out"))
            .expect("out")
            .count(),
        0
    );
}
