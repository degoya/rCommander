use std::{fs, path::Path};

use rc_vfs::{BackendId, VfsPath};

use super::{Place, place, relative, repository_root};
use crate::git::GitScope;

#[test]
fn a_git_folder_is_found_from_below() {
    let dir = tempfile::tempdir().expect("temp dir");
    let deep = dir.path().join("src/app");
    fs::create_dir_all(&deep).expect("mkdir");
    fs::create_dir(dir.path().join(".git")).expect("mkdir .git");
    assert_eq!(repository_root(&deep).as_deref(), Some(dir.path()));
    assert_eq!(repository_root(dir.path()).as_deref(), Some(dir.path()));
}

/// A linked worktree (or a submodule) has `.git` as a file pointing at the repository.
#[test]
fn a_git_file_marks_a_worktree() {
    let dir = tempfile::tempdir().expect("temp dir");
    let worktree = dir.path().join("wt");
    fs::create_dir_all(worktree.join("sub")).expect("mkdir");
    fs::write(
        worktree.join(".git"),
        "gitdir: /elsewhere/.git/worktrees/wt\n",
    )
    .expect("write");
    assert_eq!(
        repository_root(&worktree.join("sub")).as_deref(),
        Some(worktree.as_path())
    );
}

/// The nearest `.git` wins: a repository inside another one is its own.
#[test]
fn the_nearest_repository_wins() {
    let dir = tempfile::tempdir().expect("temp dir");
    let inner = dir.path().join("vendor/lib");
    fs::create_dir_all(inner.join(".git")).expect("mkdir");
    fs::create_dir(dir.path().join(".git")).expect("mkdir");
    assert_eq!(
        repository_root(&inner.join("src")).as_deref(),
        Some(inner.as_path())
    );
    assert_eq!(repository_root(&inner).as_deref(), Some(inner.as_path()));
    assert_eq!(
        repository_root(&dir.path().join("vendor")).as_deref(),
        Some(dir.path())
    );
}

/// Without `.git` the search runs up to the drive's root and finds nothing in the folders it
/// was given.
#[test]
fn outside_a_repository_nothing_is_found() {
    let dir = tempfile::tempdir().expect("temp dir");
    let deep = dir.path().join("a/b");
    fs::create_dir_all(&deep).expect("mkdir");
    let found = repository_root(&deep);
    assert!(
        found
            .as_deref()
            .is_none_or(|root| !root.starts_with(dir.path())),
        "{found:?}"
    );
}

#[test]
fn the_folder_is_named_relative_to_the_root() {
    let root = Path::new("/repo");
    assert_eq!(relative(root, Path::new("/repo")).as_deref(), Some(""));
    assert_eq!(
        relative(root, Path::new("/repo/src/app")).as_deref(),
        Some("src/app")
    );
    assert_eq!(relative(root, Path::new("/other")), None);
}

/// Only this machine's folders: local drives, a network drive with `local_network`, a WSL
/// distribution's share; never a server, nor with the switch off.
#[test]
fn git_runs_only_on_this_machine_s_folders() {
    let local = VfsPath::local("/home/me/repo");
    assert_eq!(place(&local, GitScope::Off, false), None);
    assert_eq!(place(&local, GitScope::Local, false), Some(Place::Local));
    assert_eq!(place(&local, GitScope::Local, true), None);
    assert_eq!(
        place(&local, GitScope::LocalNetwork, true),
        Some(Place::Local)
    );
    let server = VfsPath {
        backend: BackendId::Sftp { connection_id: 1 },
        path: "/srv/repo".into(),
    };
    assert_eq!(place(&server, GitScope::LocalNetwork, false), None);
    let wsl = VfsPath::local(r"\\wsl.localhost\Ubuntu\home\me\repo");
    match place(&wsl, GitScope::Local, false) {
        Some(Place::Wsl(found)) => {
            assert_eq!(found.distro, "Ubuntu");
            assert_eq!(found.linux_path, "/home/me/repo");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(place(&wsl, GitScope::Off, false), None);
}
