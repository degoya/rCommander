//! The rename that replaces (RC-110-25) against the in-process server: with
//! `posix-rename@openssh.com` the old file is replaced in one step, without it the plain
//! `RENAME` answers `AlreadyExists` and nothing moves.

use std::path::Path;

use rc_vfs::{Vfs, VfsError};

use super::two_strings;
use crate::sftp::vfs::tests::Fixture;

fn two_files(fixture: &Fixture) {
    std::fs::write(fixture.local("new.txt.rcpart"), b"new").expect("part");
    std::fs::write(fixture.local("new.txt"), b"old").expect("target");
}

#[test]
fn two_paths_are_sent_as_two_lengths_and_their_bytes() {
    assert_eq!(
        two_strings("/a", "/bcd"),
        [0, 0, 0, 2, b'/', b'a', 0, 0, 0, 4, b'/', b'b', b'c', b'd']
    );
}

#[tokio::test]
async fn a_server_with_posix_rename_replaces_the_target_in_one_step() {
    let fixture = Fixture::start(|serve| serve.posix_rename = true).await;
    two_files(&fixture);
    let (vfs, _session) = fixture.vfs().await;
    vfs.rename_replacing(Path::new("/new.txt.rcpart"), Path::new("/new.txt"))
        .await
        .expect("replaced");
    assert_eq!(
        std::fs::read(fixture.local("new.txt")).expect("target"),
        b"new"
    );
    assert!(!fixture.local("new.txt.rcpart").exists(), "the part moved");
}

#[tokio::test]
async fn a_missing_source_of_a_posix_rename_is_not_found() {
    let fixture = Fixture::start(|serve| serve.posix_rename = true).await;
    std::fs::write(fixture.local("new.txt"), b"old").expect("target");
    let (vfs, _session) = fixture.vfs().await;
    let refused = vfs
        .rename_replacing(Path::new("/gone.rcpart"), Path::new("/new.txt"))
        .await;
    assert!(
        matches!(refused, Err(VfsError::NotFound { .. })),
        "{refused:?}"
    );
    assert_eq!(
        std::fs::read(fixture.local("new.txt")).expect("target"),
        b"old"
    );
}

#[tokio::test]
async fn a_server_without_posix_rename_answers_that_the_target_exists() {
    let fixture = Fixture::start(|_| {}).await;
    two_files(&fixture);
    let (vfs, _session) = fixture.vfs().await;
    let refused = vfs
        .rename_replacing(Path::new("/new.txt.rcpart"), Path::new("/new.txt"))
        .await;
    assert!(
        matches!(refused, Err(VfsError::AlreadyExists { .. })),
        "{refused:?}"
    );
    assert_eq!(
        std::fs::read(fixture.local("new.txt")).expect("target"),
        b"old"
    );
    assert!(fixture.local("new.txt.rcpart").exists(), "nothing moved");
}
