//! The queue on an SFTP server that folds case, as OpenSSH on NTFS or APFS does (RC-080-60),
//! against the in-process server of `rc-remote` folding on any disk: the backend's probe finds it
//! out, so copying `Data` into `data/sub` is refused, a rename that only changes the case goes
//! through a temporary name, and an archive is never packed onto the server, so it can never
//! pack itself there.

mod support;

use std::fs;

use rc_ops::{Backend, JobKind, JobOptions, JobSpec, JobState, OpsError, PackSettings};
use rc_vfs::{BackendId, Vfs, VfsPath};
use support::{Harness, servers::Sftp};

const SERVER: BackendId = BackendId::Sftp { connection_id: 1 };

fn on_server(path: &str) -> VfsPath {
    VfsPath {
        backend: SERVER,
        path: path.into(),
    }
}

fn spec(kind: JobKind) -> JobSpec {
    JobSpec {
        kind,
        options: JobOptions::default(),
    }
}

/// The folding server, and the queue with this machine and the server registered.
async fn start() -> (Sftp, Harness) {
    let sftp = Sftp::folding().await;
    assert!(
        !sftp.vfs.capabilities().case_sensitive,
        "the probe found the server folds case"
    );
    let harness = Harness::start();
    harness.local();
    harness
        .queue
        .register(SERVER, Backend::new(sftp.vfs.clone()));
    (sftp, harness)
}

/// The names in the server's directory `dir`, as its disk keeps them.
fn names_on_disk(sftp: &Sftp, dir: &str) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(sftp.on_disk(dir))
        .expect("listable")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn copying_a_folder_into_itself_in_another_case_is_refused() {
    let (sftp, harness) = start().await;
    fs::create_dir_all(sftp.on_disk("/home/Data/sub")).expect("folders");
    let submitted = harness.queue.submit(spec(JobKind::Copy {
        sources: vec![on_server("/home/Data")],
        target: on_server("/home/data/sub"),
    }));
    assert!(
        matches!(submitted, Err(OpsError::TargetInsideSource { .. })),
        "{submitted:?}"
    );
}

#[tokio::test]
async fn a_rename_that_only_changes_the_case_goes_through_a_temporary_name() {
    let (sftp, harness) = start().await;
    fs::write(sftp.on_disk("/home/a.txt"), b"alpha").expect("file");
    let done = harness
        .run(spec(JobKind::MoveAs {
            source: on_server("/home/a.txt"),
            target: on_server("/home/A.TXT"),
        }))
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(names_on_disk(&sftp, "/home"), ["A.TXT"]);
    assert_eq!(
        fs::read(sftp.on_disk("/home/A.TXT")).expect("renamed"),
        b"alpha"
    );
}

#[tokio::test]
async fn an_archive_is_not_packed_onto_the_server_so_never_into_itself() {
    let (sftp, harness) = start().await;
    fs::write(sftp.on_disk("/home/archive.zip"), b"old").expect("file");
    let submitted = harness.queue.submit(spec(JobKind::Pack {
        sources: vec![on_server("/home/Archive.zip")],
        target: on_server("/home/archive.zip"),
        settings: PackSettings::default(),
    }));
    let error = submitted.expect_err("refused when submitted");
    assert_eq!(error.code(), "unsupported", "{error}");
    assert_eq!(
        fs::read(sftp.on_disk("/home/archive.zip")).expect("kept"),
        b"old"
    );
}
