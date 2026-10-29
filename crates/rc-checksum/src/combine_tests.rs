use std::path::PathBuf;

use rc_vfs::LocalFs;

use super::*;
use crate::{
    BUFFER_SIZE, split,
    test_support::{self, Tripwire, names},
};

/// A file split into `parts/`, an empty `back/` to combine into, and the data.
async fn prepared(size: usize, part_size: u64) -> (tempfile::TempDir, Arc<dyn Vfs>, Vec<u8>) {
    let (root, vfs) = test_support::local();
    let data = test_support::pattern(size);
    test_support::write(root.path(), "data.bin", &data);
    std::fs::create_dir(root.path().join("parts")).expect("parts");
    std::fs::create_dir(root.path().join("back")).expect("back");
    split(
        &vfs,
        &root.path().join("data.bin"),
        &vfs,
        &root.path().join("parts"),
        part_size,
        &Control::new(),
    )
    .await
    .expect("split");
    (root, vfs, data)
}

fn dirs(root: &tempfile::TempDir) -> (PathBuf, PathBuf) {
    (root.path().join("parts"), root.path().join("back"))
}

async fn run(
    vfs: &Arc<dyn Vfs>,
    first: &Path,
    back: &Path,
    name: Option<&str>,
) -> Result<CombineReport, ChecksumError> {
    combine(vfs, first, vfs, back, name, &Control::new()).await
}

#[tokio::test]
async fn a_missing_part_stops_before_anything_is_written() {
    let (root, vfs, _) = prepared(10_000, 3_000).await;
    let (parts, back) = dirs(&root);
    std::fs::remove_file(parts.join("data.002")).expect("remove");
    let error = run(&vfs, &parts.join("data.001"), &back, None)
        .await
        .expect_err("missing");
    assert!(
        matches!(&error, ChecksumError::PartMissing { name } if name == "data.002"),
        "{error:?}"
    );
    // The last part gone: only the .crc file's size can tell.
    test_support::write(&parts, "data.002", &[0; 3_000]);
    std::fs::remove_file(parts.join("data.004")).expect("remove");
    let error = run(&vfs, &parts.join("data.001"), &back, None)
        .await
        .expect_err("missing");
    assert!(
        matches!(&error, ChecksumError::PartMissing { name } if name == "data.004"),
        "{error:?}"
    );
    assert!(names(&back).is_empty());
}

#[tokio::test]
async fn a_damaged_part_fails_the_crc_and_leaves_no_target() {
    let (root, vfs, _) = prepared(10_000, 3_000).await;
    let (parts, back) = dirs(&root);
    test_support::write(&parts, "data.002", &[0; 3_000]);
    let error = run(&vfs, &parts.join("data.001"), &back, None)
        .await
        .expect_err("crc");
    assert_eq!(error.code(), "combine_crc_mismatch");
    assert!(names(&back).is_empty());
    test_support::write(&parts, "data.004", &[0; 1_001]);
    let error = run(&vfs, &parts.join("data.001"), &back, None)
        .await
        .expect_err("size");
    assert_eq!(error.code(), "combine_size_mismatch");
}

/// The local disk, except that every listed file claims half of `u64`: a server's sizes.
struct HugeSizes(LocalFs);

#[async_trait::async_trait]
impl rc_vfs::testing::Forward for HugeSizes {
    fn inner(&self) -> &dyn Vfs {
        &self.0
    }
    async fn list(&self, dir: &Path) -> rc_vfs::Result<Vec<rc_vfs::Entry>> {
        let mut entries = self.0.list(dir).await?;
        for entry in &mut entries {
            entry.size = u64::MAX / 2 + 1;
        }
        Ok(entries)
    }
}

#[tokio::test]
async fn listed_sizes_that_overflow_are_an_error_not_a_wrap() {
    let (root, _, _) = prepared(10_000, 3_000).await;
    let (parts, back) = dirs(&root);
    let vfs: Arc<dyn Vfs> = Arc::new(HugeSizes(LocalFs::new()));
    let error = run(&vfs, &parts.join("data.001"), &back, None)
        .await
        .expect_err("overflow");
    assert_eq!(error.code(), "io", "{error:?}");
    assert!(names(&back).is_empty());
}

#[tokio::test]
async fn without_a_crc_file_the_parts_are_combined_unchecked() {
    let (root, vfs, data) = prepared(10_000, 3_000).await;
    let (parts, back) = dirs(&root);
    std::fs::remove_file(parts.join("data.crc")).expect("remove");
    let report = run(&vfs, &parts.join("data.001"), &back, None)
        .await
        .expect("combined");
    assert_eq!(report.target, "data");
    assert!(!report.crc_checked);
    assert_eq!(std::fs::read(back.join("data")).expect("back"), data);
    let named = run(&vfs, &parts.join("data.001"), &back, Some("named.bin"))
        .await
        .expect("combined");
    assert_eq!(named.target, "named.bin");
}

#[tokio::test]
async fn an_existing_target_bad_names_and_other_files_are_refused() {
    let (root, vfs, _) = prepared(10_000, 3_000).await;
    let (parts, back) = dirs(&root);
    test_support::write(&back, "data.bin", b"mine");
    let code = |result: Result<CombineReport, ChecksumError>| result.expect_err("refused").code();
    let first = parts.join("data.001");
    assert_eq!(code(run(&vfs, &first, &back, None).await), "already_exists");
    assert_eq!(
        code(run(&vfs, &first, &back, Some("../x")).await),
        "invalid_name"
    );
    assert_eq!(
        code(run(&vfs, &root.path().join("data.bin"), &back, None).await),
        "combine_not_a_split_file"
    );
    test_support::write(&parts, "data.crc", b"filename=../../evil\r\ncrc32=0\r\n");
    assert_eq!(
        code(run(&vfs, &first, &back, None).await),
        "split_crc_file_invalid"
    );
    assert_eq!(names(&back), vec!["data.bin"]);
    assert_eq!(std::fs::read(back.join("data.bin")).expect("kept"), b"mine");
}

#[tokio::test]
async fn a_cancel_half_way_removes_the_target() {
    let (root, _, _) = prepared(4 * BUFFER_SIZE, BUFFER_SIZE as u64).await;
    let (parts, back) = dirs(&root);
    let control = Control::new();
    let vfs: Arc<dyn Vfs> = Arc::new(Tripwire {
        inner: LocalFs::new(),
        control: control.clone(),
        after: BUFFER_SIZE as u64 / 2,
    });
    let error = combine(&vfs, &parts.join("data.001"), &vfs, &back, None, &control)
        .await
        .expect_err("cancelled");
    assert_eq!(error.code(), "cancelled");
    assert!(names(&back).is_empty());
    // The File bar (RC-010-07): the first part is the file at work, with its own size.
    assert_eq!(control.current_file(), Some(parts.join("data.001")));
    assert_eq!(control.file_bytes_total(), BUFFER_SIZE as u64);
    assert!(control.file_bytes_done() > 0);
}

#[test]
fn only_parts_and_crc_files_have_a_stem() {
    assert_eq!(split_stem("disk.001"), Some("disk"));
    assert_eq!(split_stem("disk.iso.042"), Some("disk.iso"));
    assert_eq!(split_stem("disk.CRC"), Some("disk"));
    assert_eq!(split_stem("disk.iso"), None);
    assert_eq!(split_stem("disk.01"), None);
    assert_eq!(split_stem(".001"), None);
}
