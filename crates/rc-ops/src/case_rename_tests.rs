use std::path::Path;

use rc_vfs::{MemoryFs, WriteMode, conformance};

use super::*;

#[tokio::test]
async fn a_temporary_name_is_free_and_beside_the_entry() {
    let fs = MemoryFs::new();
    let temp = free_temp_name(&fs, Path::new("/dir/a.txt"))
        .await
        .expect("a free name");
    assert_eq!(temp.parent(), Some(Path::new("/dir")));
    let name = temp.file_name().expect("a name").to_string_lossy();
    assert!(name.starts_with(".rc-tmp-"), "{name}");
}

async fn file(fs: &MemoryFs, path: &str, bytes: &[u8]) {
    conformance::write_file(fs, Path::new(path), WriteMode::Truncate, bytes).await;
}

/// RC-110-22: the backend tells case apart, so the target is another file - it stays, and the
/// entry is back under its old name with nothing left at a temporary one.
#[tokio::test]
async fn a_taken_target_is_never_replaced_and_the_old_name_comes_back() {
    let fs = MemoryFs::new();
    fs.mkdir(Path::new("/dir"), true).await.expect("mkdir");
    file(&fs, "/dir/makefile", b"mine").await;
    file(&fs, "/dir/Makefile", b"theirs").await;
    let mut parked = None;
    let refused = two_steps(
        &fs,
        Path::new("/dir/makefile"),
        Path::new("/dir/Makefile"),
        &mut parked,
    )
    .await;
    assert!(
        matches!(
            refused,
            Err(Step::Fail(OpsError::Vfs(VfsError::AlreadyExists { .. })))
        ),
        "{refused:?}"
    );
    assert_eq!(parked, None);
    let read = |path: &'static str| conformance::read_file(&fs, Path::new(path));
    assert_eq!(read("/dir/makefile").await, b"mine");
    assert_eq!(read("/dir/Makefile").await, b"theirs");
    assert_eq!(fs.list(Path::new("/dir")).await.expect("list").len(), 2);
}

/// The undo does not replace either: a file that took the old name meanwhile stays, and the
/// entry is reported under its temporary name.
#[tokio::test]
async fn an_undo_onto_a_name_taken_meanwhile_leaves_the_entry_parked() {
    let fs = MemoryFs::new();
    fs.mkdir(Path::new("/dir"), true).await.expect("mkdir");
    file(&fs, "/dir/Makefile", b"theirs").await;
    file(&fs, "/dir/makefile", b"newer").await;
    file(&fs, "/dir/.rc-tmp-x", b"mine").await;
    // The first step is done: the entry is at its temporary name, and both names are taken.
    let mut parked = Some(PathBuf::from("/dir/.rc-tmp-x"));
    let refused = two_steps(
        &fs,
        Path::new("/dir/makefile"),
        Path::new("/dir/Makefile"),
        &mut parked,
    )
    .await;
    assert!(
        matches!(refused, Err(Step::Fail(OpsError::RenameUndoFailed { .. }))),
        "{refused:?}"
    );
    assert_eq!(parked, Some(PathBuf::from("/dir/.rc-tmp-x")));
    let read = |path: &'static str| conformance::read_file(&fs, Path::new(path));
    assert_eq!(read("/dir/makefile").await, b"newer");
    assert_eq!(read("/dir/Makefile").await, b"theirs");
    assert_eq!(read("/dir/.rc-tmp-x").await, b"mine");
}

#[test]
fn another_case_is_the_same_entry_only_where_case_does_not_count() {
    let (lower, upper) = (Path::new("/data/readme"), Path::new("/data/README"));
    assert!(only_case(false, lower, upper));
    assert!(
        !only_case(true, lower, upper),
        "the backend tells case apart"
    );
    assert!(
        !only_case(false, lower, lower),
        "the same path is no rename"
    );
    assert!(!only_case(false, lower, Path::new("/data/other")));
    // A WSL folder is a Linux file system behind a backend that says case does not count.
    let wsl = |name: &str| format!(r"\\wsl.localhost\Ubuntu\home\me\{name}");
    assert!(!only_case(
        false,
        Path::new(&wsl("makefile")),
        Path::new(&wsl("Makefile"))
    ));
}
