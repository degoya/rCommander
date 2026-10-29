//! What only Windows has: paths past the 260-character limit.

use std::path::PathBuf;

use super::{by_name, write};
use crate::{EntryKind, LocalFs, Vfs, WriteMode, conformance};

/// A directory chain under `root` whose file path is well past 260 characters.
fn deep_under(root: &std::path::Path) -> (PathBuf, PathBuf) {
    let mut dir = root.to_path_buf();
    for level in 0..8 {
        dir.push(format!("{level}-a-directory-name-of-forty-characters--"));
    }
    let file = dir.join("a-file-at-the-end-of-a-long-path.txt");
    assert!(
        file.as_os_str().len() > 300,
        "{} is only {} characters",
        file.display(),
        file.as_os_str().len()
    );
    (dir, file)
}

#[tokio::test]
async fn a_path_past_260_characters_round_trips() {
    let root = tempfile::tempdir().expect("tempdir");
    let fs = LocalFs::new();
    let (dir, file) = deep_under(root.path());

    fs.mkdir(&dir, true).await.expect("mkdir a long chain");
    write(&fs, &file, WriteMode::CreateNew, b"deep").await;
    assert_eq!(conformance::read_file(&fs, &file).await, b"deep");

    let entry = fs.stat(&file).await.expect("stat a long path");
    assert_eq!(entry.kind, EntryKind::File);
    assert_eq!(entry.size, 4);
    let listed = by_name(fs.list(&dir).await.expect("list a long path"));
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "a-file-at-the-end-of-a-long-path.txt");

    let renamed = dir.join("renamed-at-the-end-of-a-long-path.txt");
    fs.rename(&file, &renamed)
        .await
        .expect("rename a long path");
    fs.remove_file(&renamed).await.expect("remove a long path");
    fs.remove_dir(
        &root.path().join("0-a-directory-name-of-forty-characters--"),
        true,
    )
    .await
    .expect("remove a long chain");
    assert!(fs.list(root.path()).await.expect("list").is_empty());
}
