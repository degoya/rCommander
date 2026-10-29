use std::path::Path;

use super::{Forward, TRICKLE_CHUNK, Trickle};
use crate::{MemoryFs, Vfs, VfsError, WriteMode, conformance};

/// Overrides nothing.
struct Plain(MemoryFs);

impl Forward for Plain {
    fn inner(&self) -> &dyn Vfs {
        &self.0
    }
}

/// The suite's checks of the calls that forward; `write_modes` reads from an offset, which a
/// plain forward refuses as `Vfs`'s default does.
#[tokio::test]
async fn a_plain_forward_answers_as_its_inner_backend() {
    let fs = Plain(MemoryFs::new());
    let vfs: &dyn Vfs = &fs;
    for dir in [
        "/listing", "/stat", "/mkdir", "/rename", "/remove", "/mtime", "/case",
    ] {
        vfs.mkdir(Path::new(dir), false).await.expect("mkdir");
    }
    conformance::listing_describes_files_and_directories(vfs, Path::new("/listing")).await;
    conformance::stat_describes_one_entry(vfs, Path::new("/stat")).await;
    conformance::mkdir_with_and_without_parents(vfs, Path::new("/mkdir")).await;
    conformance::rename_within_the_backend(vfs, Path::new("/rename")).await;
    conformance::remove_files_and_directories(vfs, Path::new("/remove")).await;
    conformance::set_mtime_is_what_stat_reads_back(vfs, Path::new("/mtime")).await;
    conformance::case_follows_the_capability(vfs, Path::new("/case")).await;
}

#[tokio::test]
async fn the_provided_calls_keep_the_trait_defaults() {
    let memory = MemoryFs::new();
    conformance::write_file(&memory, Path::new("/f"), WriteMode::CreateNew, b"abcdef").await;
    let vfs: &dyn Vfs = &Plain(memory);
    let at = vfs.open_read_at(Path::new("/f"), 2).await;
    assert!(matches!(at, Err(VfsError::Unsupported { .. })));
    let seek = vfs.open_read_seekable(Path::new("/f")).await;
    assert!(matches!(seek, Err(VfsError::Unsupported { .. })));
    assert_eq!(vfs.free_space(Path::new("/")).await.expect("space"), None);
    assert_eq!(
        conformance::read_file(vfs, Path::new("/f")).await,
        b"abcdef"
    );
}

#[tokio::test]
async fn a_trickle_reads_the_whole_file() {
    let root = tempfile::tempdir().expect("temp dir");
    let file = root.path().join("big.bin");
    let bytes: Vec<u8> = (0..TRICKLE_CHUNK * 2 + 7)
        .map(|at| (at % 251) as u8)
        .collect();
    std::fs::write(&file, &bytes).expect("write");
    let vfs: &dyn Vfs = &Trickle::default();
    assert_eq!(conformance::read_file(vfs, &file).await, bytes);
}
