//! F5 and F6 between this machine and an SFTP server, end to end against the in-process SSH
//! server of `rc-remote` (RC-060-05): a folder up and back byte for byte, times kept, an
//! overwrite through a server whose rename does not replace, a cancel inside a file and a move.

mod support;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

use rc_ops::{Backend, ConflictChoice, JobChange, JobKind, JobOptions, JobSpec, JobState};
use rc_remote::{
    SftpVfs, SshSession,
    sftp::SftpLimits,
    ssh::{SessionOptions, SshTarget},
    testkit::{
        MemoryCredentials, MemoryGate, PASSWORD, ServerOptions, SftpServe, TestServer, host_key_a,
    },
};
use rc_vfs::{BackendId, VfsPath};
use support::{Harness, payload};
use tokio_util::sync::CancellationToken;

const SERVER: BackendId = BackendId::Sftp { connection_id: 1 };
const BIG: usize = 1 << 20;

/// The server, the directory it serves as `/`, the session the backend runs on, a local
/// directory, and the queue with this machine and the server registered.
struct Fixture {
    _server: TestServer,
    served: tempfile::TempDir,
    _session: SshSession,
    here: tempfile::TempDir,
    harness: Harness,
}

impl Fixture {
    async fn start() -> Self {
        let served = tempfile::tempdir().expect("served directory");
        let server = TestServer::start(ServerOptions {
            sftp: Some(SftpServe::new(served.path())),
            ..ServerOptions::new(host_key_a())
        })
        .await;
        let target = SshTarget {
            host: "127.0.0.1".to_owned(),
            port: server.port,
            user: "tester".to_owned(),
        };
        let credentials = MemoryCredentials {
            password: Some(PASSWORD.to_owned()),
            ..MemoryCredentials::default()
        };
        let session = SshSession::connect(
            &target,
            &SessionOptions::default(),
            Arc::new(MemoryGate::trusting(&host_key_a())),
            &credentials,
            &CancellationToken::new(),
        )
        .await
        .expect("session");
        let names = session.open_channel().await.expect("channel");
        let files = session.open_channel().await.expect("channel");
        let sftp = SftpVfs::connect(names, files, SftpLimits::default())
            .await
            .expect("sftp");
        let harness = Harness::start();
        harness.local();
        harness.queue.register(SERVER, Backend::new(Arc::new(sftp)));
        Self {
            _server: server,
            served,
            _session: session,
            here: tempfile::tempdir().expect("local directory"),
            harness,
        }
    }

    /// `path` on this machine, below the local directory.
    fn here(&self, path: &str) -> PathBuf {
        self.here.path().join(path)
    }

    /// The file the server keeps for its path `path`.
    fn on_disk(&self, path: &str) -> PathBuf {
        self.served.path().join(path.trim_start_matches('/'))
    }

    async fn run(&self, kind: JobKind, options: JobOptions) -> rc_ops::JobSnapshot {
        self.harness.run(JobSpec { kind, options }).await
    }
}

fn on_server(path: &str) -> VfsPath {
    VfsPath {
        backend: SERVER,
        path: path.into(),
    }
}

fn hash(path: &Path) -> blake3::Hash {
    blake3::hash(&fs::read(path).expect("readable"))
}

/// Every file below `dir`, relative, with its checksum.
fn files(dir: &Path) -> Vec<(PathBuf, blake3::Hash)> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in fs::read_dir(&next).expect("listable") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path.strip_prefix(dir).expect("below").to_path_buf();
                found.push((rel, hash(&path)));
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

fn modified(path: &Path) -> SystemTime {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .expect("a time")
}

#[tokio::test]
async fn a_folder_goes_up_and_comes_back_byte_for_byte() {
    let fixture = Fixture::start().await;
    let src = fixture.here("src");
    fs::create_dir_all(src.join("sub/deeper")).expect("mkdir");
    fs::create_dir_all(src.join("empty")).expect("mkdir");
    fs::write(src.join("a.txt"), b"alpha").expect("write");
    fs::write(src.join("sub/b.bin"), payload(BIG, 3)).expect("write");
    fs::write(src.join("sub/deeper/c.bin"), payload(7000, 9)).expect("write");
    let when = SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    fs::File::options()
        .write(true)
        .open(src.join("a.txt"))
        .and_then(|file| file.set_modified(when))
        .expect("set mtime");

    let up = fixture
        .run(
            JobKind::Copy {
                sources: vec![VfsPath::local(&src)],
                target: on_server("/up"),
            },
            JobOptions::default(),
        )
        .await;
    assert_eq!(up.state, JobState::Completed, "{up:?}");
    assert_eq!(files(&fixture.on_disk("/up/src")), files(&src));
    assert!(fixture.on_disk("/up/src/empty").is_dir());
    assert_eq!(modified(&fixture.on_disk("/up/src/a.txt")), when);

    let back = fixture.here("back");
    let down = fixture
        .run(
            JobKind::Copy {
                sources: vec![on_server("/up/src")],
                target: VfsPath::local(&back),
            },
            JobOptions::default(),
        )
        .await;
    assert_eq!(down.state, JobState::Completed, "{down:?}");
    assert_eq!(files(&back.join("src")), files(&src), "byte for byte");
    assert_eq!(
        modified(&back.join("src/a.txt")),
        when,
        "the time came back"
    );
}

#[tokio::test]
async fn an_overwrite_replaces_the_file_on_a_server_that_does_not_replace_on_rename() {
    let fixture = Fixture::start().await;
    fs::create_dir_all(fixture.on_disk("/up")).expect("mkdir");
    fs::write(fixture.on_disk("/up/a.txt"), b"the old one").expect("write");
    let source = fixture.here("a.txt");
    fs::write(&source, b"the new one").expect("write");
    let done = fixture
        .run(
            JobKind::Copy {
                sources: vec![VfsPath::local(&source)],
                target: on_server("/up"),
            },
            JobOptions {
                on_conflict: Some(ConflictChoice::Overwrite),
                ..JobOptions::default()
            },
        )
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(
        fs::read(fixture.on_disk("/up/a.txt")).expect("read"),
        b"the new one"
    );
    assert!(!fixture.on_disk("/up/a.txt.rcpart").exists());
}

#[tokio::test]
async fn a_cancel_inside_an_upload_leaves_nothing_under_the_name() {
    let fixture = Fixture::start().await;
    let source = fixture.here("big.bin");
    fs::write(&source, payload(BIG, 5)).expect("write");
    fixture.harness.on_event(|queue, event| {
        if let JobChange::Progress { progress } = &event.change
            && progress.current.is_some()
            && progress.bytes_done > 0
        {
            let _ = queue.cancel(event.job);
        }
    });
    let done = fixture
        .run(
            JobKind::Copy {
                sources: vec![VfsPath::local(&source)],
                target: on_server("/up"),
            },
            JobOptions::default(),
        )
        .await;
    assert_eq!(done.state, JobState::Cancelled, "{done:?}");
    assert!(!fixture.on_disk("/up/big.bin").exists());
    assert!(!fixture.on_disk("/up/big.bin.rcpart").exists(), "removed");
    assert!(source.exists());
}

#[tokio::test]
async fn a_move_to_the_server_removes_the_source_once_it_is_there() {
    let fixture = Fixture::start().await;
    let source = fixture.here("b.bin");
    fs::write(&source, payload(BIG, 7)).expect("write");
    let done = fixture
        .run(
            JobKind::Move {
                sources: vec![VfsPath::local(&source)],
                target: on_server("/up"),
            },
            JobOptions::default(),
        )
        .await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(
        fs::read(fixture.on_disk("/up/b.bin")).expect("read"),
        payload(BIG, 7)
    );
    assert!(!source.exists(), "the source went after the copy");
}
