//! `SftpVfs` against the in-process server: the `rc-vfs` conformance suite, streaming between two
//! sessions, the home directory, links, hostile and huge listings, a stalled server, refused
//! rights and a server without SFTP.

use std::{path::Path, sync::Arc, time::Duration};

use rc_vfs::{Vfs, conformance};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::{
    SshSession,
    sftp::SftpError,
    ssh::{SessionOptions, SshTarget},
    testkit::{
        MemoryCredentials, MemoryGate, PASSWORD, ServerOptions, SftpServe, TestServer, host_key_a,
    },
};

/// A server, the directory it serves as `/`, and the session the backend runs on.
struct Fixture {
    server: TestServer,
    root: tempfile::TempDir,
}

impl Fixture {
    async fn start(adjust: impl FnOnce(&mut SftpServe)) -> Self {
        let root = tempfile::tempdir().expect("served directory");
        let mut serve = SftpServe::new(root.path());
        adjust(&mut serve);
        let server = TestServer::start(ServerOptions {
            sftp: Some(serve),
            ..ServerOptions::new(host_key_a())
        })
        .await;
        Self { server, root }
    }

    fn local(&self, name: &str) -> std::path::PathBuf {
        self.root.path().join(name)
    }

    async fn session(&self) -> SshSession {
        let target = SshTarget {
            host: "127.0.0.1".to_owned(),
            port: self.server.port,
            user: "tester".to_owned(),
        };
        let credentials = MemoryCredentials {
            password: Some(PASSWORD.to_owned()),
            ..MemoryCredentials::default()
        };
        SshSession::connect(
            &target,
            &SessionOptions::default(),
            Arc::new(MemoryGate::trusting(&host_key_a())),
            &credentials,
            &CancellationToken::new(),
        )
        .await
        .expect("session")
    }

    /// A backend with `limits` on a session of its own, which the caller keeps.
    async fn vfs_with(&self, limits: SftpLimits) -> (SftpVfs, SshSession) {
        let session = self.session().await;
        let names = session.open_channel().await.expect("channel");
        let files = session.open_channel().await.expect("channel");
        let vfs = SftpVfs::connect(names, files, limits).await.expect("sftp");
        (vfs, session)
    }

    async fn vfs(&self) -> (SftpVfs, SshSession) {
        self.vfs_with(SftpLimits::default()).await
    }
}

#[tokio::test]
async fn sftp_conforms() {
    let fixture = Fixture::start(|_| {}).await;
    std::fs::create_dir(fixture.local("conformance")).expect("root");
    let (vfs, _session) = fixture.vfs().await;
    conformance::run(&vfs, Path::new("/conformance")).await;
}

#[tokio::test]
async fn files_stream_between_two_sessions_and_read_from_an_offset() {
    let fixture = Fixture::start(|_| {}).await;
    for dir in ["from", "to"] {
        std::fs::create_dir(fixture.local(dir)).expect("dir");
    }
    let (from, _one) = fixture.vfs().await;
    let (to, _two) = fixture.vfs().await;
    conformance::copy_between(&from, Path::new("/from"), &to, Path::new("/to")).await;

    std::fs::write(fixture.local("resume.bin"), b"0123456789").expect("file");
    let mut stream = from
        .open_read_at(Path::new("/resume.bin"), 4)
        .await
        .expect("open at 4");
    let mut rest = Vec::new();
    stream.read_to_end(&mut rest).await.expect("read");
    assert_eq!(rest, b"456789");
    // The conformance check reads `hello` from its second byte on.
    std::fs::write(fixture.local("hello.txt"), b"hello").expect("file");
    conformance::seekable_read_when_offered(&from, Path::new("/hello.txt")).await;
}

#[tokio::test]
async fn the_home_directory_is_where_the_server_says() {
    let fixture = Fixture::start(|serve| serve.home = "/home/tester".to_owned()).await;
    std::fs::create_dir_all(fixture.local("home/tester")).expect("home");
    let (vfs, _session) = fixture.vfs().await;
    assert_eq!(vfs.home(), Path::new("/home/tester"));
}

#[tokio::test]
async fn names_outside_the_directory_are_dropped_from_a_listing() {
    let hostile = [
        "../escape",
        "a/b",
        "c\\d",
        "",
        ".",
        "..",
        "line\nbreak",
        "nul\0",
    ];
    let fixture = Fixture::start(|serve| {
        serve.hostile_names = hostile.iter().map(|name| (*name).to_owned()).collect();
    })
    .await;
    std::fs::write(fixture.local("real.txt"), b"x").expect("file");
    let (vfs, _session) = fixture.vfs().await;
    let names: Vec<String> = vfs
        .list(Path::new("/"))
        .await
        .expect("list")
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(names, ["real.txt"]);
}

#[tokio::test]
async fn a_large_directory_is_read_block_by_block_and_capped() {
    let fixture = Fixture::start(|serve| serve.block = 37).await;
    std::fs::create_dir(fixture.local("many")).expect("dir");
    for index in 0..1000 {
        std::fs::write(fixture.local(&format!("many/f{index:04}")), b"").expect("file");
    }
    let (vfs, _session) = fixture.vfs().await;
    assert_eq!(
        vfs.list(Path::new("/many")).await.expect("list").len(),
        1000
    );

    let (capped, _other) = fixture
        .vfs_with(SftpLimits {
            max_entries: 500,
            ..SftpLimits::default()
        })
        .await;
    let error = capped.list(Path::new("/many")).await.expect_err("too many");
    let failure = SftpError::from_vfs(&error).expect("a session failure");
    assert_eq!(failure.code(), "sftp_listing_too_large");
    assert!(
        !capped.is_closed(),
        "a refused listing leaves the session usable"
    );
}

/// RC-060-02: a listing arrives block by block, and one nobody waits for any more ends early
/// with the handle closed.
#[tokio::test]
async fn a_listing_arrives_in_blocks_and_stops_when_nobody_listens() {
    let fixture = Fixture::start(|serve| serve.block = 37).await;
    std::fs::create_dir(fixture.local("many")).expect("dir");
    for index in 0..200 {
        std::fs::write(fixture.local(&format!("many/f{index:04}")), b"").expect("file");
    }
    let (vfs, _session) = fixture.vfs().await;
    let dir = Path::new("/many");

    let (sender, mut receiver) = tokio::sync::mpsc::channel(64);
    vfs.list_batches(dir, sender).await.expect("listed");
    let mut sizes = Vec::new();
    while let Some(batch) = receiver.recv().await {
        sizes.push(batch.len());
    }
    assert_eq!(sizes.iter().sum::<usize>(), 200, "{sizes:?}");
    assert!(sizes.len() > 1, "one batch per block: {sizes:?}");

    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let first = async {
        let first = receiver.recv().await;
        drop(receiver);
        first
    };
    let (listed, first) = tokio::join!(vfs.list_batches(dir, sender), first);
    listed.expect("a listing nobody waits for ends without an error");
    let first = first.expect("a first block").len();
    assert!(first > 0 && first < 200, "{first}");
    assert!(!vfs.is_closed());
    assert_eq!(vfs.list(dir).await.expect("listed again").len(), 200);
}

/// RC-060-02 acceptance: a folder of 100 000 entries on the test server hands out its first
/// block while the listing is far from its end, and every entry arrives.
#[tokio::test]
async fn a_folder_of_100_000_entries_shows_its_first_block_before_the_end() {
    const ENTRIES: usize = 100_000;
    let fixture = Fixture::start(|serve| serve.block = 1000).await;
    std::fs::create_dir(fixture.local("huge")).expect("dir");
    for index in 0..ENTRIES {
        std::fs::File::create(fixture.local(&format!("huge/f{index:06}"))).expect("file");
    }
    let (vfs, _session) = fixture.vfs().await;
    let ended = std::sync::atomic::AtomicBool::new(false);
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let listing = async {
        let listed = vfs.list_batches(Path::new("/huge"), sender).await;
        ended.store(true, std::sync::atomic::Ordering::SeqCst);
        listed
    };
    let taking = async {
        let first = receiver.recv().await.expect("a first block").len();
        let early = !ended.load(std::sync::atomic::Ordering::SeqCst);
        let mut total = first;
        while let Some(batch) = receiver.recv().await {
            total += batch.len();
        }
        (first, early, total)
    };
    let (listed, (first, early, total)) = tokio::join!(listing, taking);
    listed.expect("listed");
    assert!(early, "the first block came before the listing ended");
    assert!(first < ENTRIES, "{first}");
    assert_eq!(total, ENTRIES);
}

/// RC-060-02: the owner comes from the `longname`; the test server writes the numbers there.
#[cfg(unix)]
#[tokio::test]
async fn a_listing_names_the_owner_from_the_longname() {
    use std::os::unix::fs::MetadataExt;

    let fixture = Fixture::start(|_| {}).await;
    std::fs::write(fixture.local("owned"), b"x").expect("file");
    let meta = std::fs::metadata(fixture.local("owned")).expect("metadata");
    let (vfs, _session) = fixture.vfs().await;

    let listed = vfs.list(Path::new("/")).await.expect("list");
    let owner = listed[0].owner.clone().expect("an owner");
    assert_eq!(owner.user, meta.uid().to_string());
    assert_eq!(owner.group, meta.gid().to_string());
    let stat = vfs.stat(Path::new("/owned")).await.expect("stat");
    assert_eq!(
        stat.owner,
        Some(owner),
        "stat reads the numbers from the attributes"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_link_to_a_directory_is_listed_as_a_link_and_can_be_entered() {
    use rc_vfs::{EntryKind, TargetKind};

    let fixture = Fixture::start(|_| {}).await;
    std::fs::create_dir(fixture.local("real")).expect("dir");
    std::fs::write(fixture.local("real/inside.txt"), b"in").expect("file");
    std::os::unix::fs::symlink("real", fixture.local("link")).expect("link");
    std::os::unix::fs::symlink("gone", fixture.local("dangling")).expect("link");
    let (vfs, _session) = fixture.vfs().await;

    let mut entries = vfs.list(Path::new("/")).await.expect("list");
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let kinds: Vec<(&str, &EntryKind)> = entries
        .iter()
        .map(|entry| (entry.name.as_str(), &entry.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            (
                "dangling",
                &EntryKind::Symlink {
                    target: "gone".into(),
                    target_kind: None
                }
            ),
            (
                "link",
                &EntryKind::Symlink {
                    target: "real".into(),
                    target_kind: Some(TargetKind::Dir)
                }
            ),
            ("real", &EntryKind::Dir),
        ]
    );
    assert!(
        vfs.stat(Path::new("/link"))
            .await
            .expect("stat")
            .is_dir_like()
    );
    let inside = vfs.list(Path::new("/link")).await.expect("enter the link");
    assert_eq!(inside[0].name, "inside.txt");
    vfs.remove_file(Path::new("/link"))
        .await
        .expect("remove the link");
    assert!(
        fixture.local("real/inside.txt").exists(),
        "the target stays"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_refused_change_is_permission_denied() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::start(|_| {}).await;
    std::fs::create_dir(fixture.local("locked")).expect("dir");
    std::fs::set_permissions(
        fixture.local("locked"),
        std::fs::Permissions::from_mode(0o555),
    )
    .expect("chmod");
    let (vfs, _session) = fixture.vfs().await;
    let result = vfs.mkdir(Path::new("/locked/new"), false).await;
    std::fs::set_permissions(
        fixture.local("locked"),
        std::fs::Permissions::from_mode(0o755),
    )
    .expect("chmod back");
    match result {
        // Root writes anywhere: nothing to check then.
        Ok(()) => {}
        Err(error) => assert_eq!(error.code(), "permission_denied", "{error}"),
    }
    let entry = vfs.stat(Path::new("/locked")).await.expect("stat");
    assert_eq!(entry.attributes.mode, Some(0o755));
}

#[tokio::test]
async fn a_stalled_server_times_out_without_a_panic_and_closes_the_backend() {
    let fixture = Fixture::start(|_| {}).await;
    let (vfs, _session) = fixture
        .vfs_with(SftpLimits {
            request_timeout: Duration::from_secs(1),
            ..SftpLimits::default()
        })
        .await;
    vfs.list(Path::new("/"))
        .await
        .expect("list while it answers");
    fixture.server.freeze();
    let error = vfs.list(Path::new("/")).await.expect_err("no answer");
    let failure = SftpError::from_vfs(&error).expect("a session failure");
    assert_eq!(failure.code(), "sftp_timeout");
    assert!(vfs.is_closed(), "the next use opens a new backend");
}

/// RC-060-01: a backend that watches its session is closed once the session ends, before any
/// call has to find out - the next use opens a new one.
#[tokio::test]
async fn a_backend_watching_its_session_is_closed_when_the_session_ends() {
    let fixture = Fixture::start(|_| {}).await;
    let (vfs, session) = fixture.vfs().await;
    let vfs = vfs.ending_with(session.closed_signal());
    vfs.list(Path::new("/")).await.expect("list");
    assert!(!vfs.is_closed());
    session.disconnect().await;
    tokio::time::timeout(Duration::from_secs(5), session.closed())
        .await
        .expect("the session ends");
    assert!(vfs.is_closed(), "closed without a call");
}

/// RC-080-04: the session ending while a listing waits for the server fails the listing at
/// once with `connection_lost`, not after the request timeout as `sftp_timeout`.
#[tokio::test]
async fn a_listing_waiting_when_the_session_ends_fails_at_once_as_connection_lost() {
    let fixture = Fixture::start(|_| {}).await;
    let (vfs, session) = fixture.vfs().await;
    let vfs = vfs.ending_with(session.closed_signal());
    vfs.list(Path::new("/"))
        .await
        .expect("list while it answers");
    fixture.server.freeze();
    let started = std::time::Instant::now();
    let (listed, ()) = tokio::join!(vfs.list(Path::new("/")), async {
        tokio::time::sleep(Duration::from_millis(200)).await;
        session.disconnect().await;
    });
    let error = listed.expect_err("the session ended");
    let failure = SftpError::from_vfs(&error).expect("a session failure");
    assert_eq!(failure.code(), "connection_lost");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "at once, not after {:?}",
        started.elapsed()
    );
    assert!(vfs.is_closed());
}

#[tokio::test]
async fn a_server_without_sftp_is_unavailable() {
    let server = TestServer::start(ServerOptions::new(host_key_a())).await;
    let fixture = Fixture {
        server,
        root: tempfile::tempdir().expect("dir"),
    };
    let session = fixture.session().await;
    let names = session.open_channel().await.expect("channel");
    let files = session.open_channel().await.expect("channel");
    let error = SftpVfs::connect(names, files, SftpLimits::default())
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "sftp_unavailable");
}

// Unix modes: a Windows directory served knows only read-only.
#[cfg(unix)]
#[tokio::test]
async fn writing_modes_and_attributes_reach_the_server() {
    let fixture = Fixture::start(|_| {}).await;
    let (vfs, _session) = fixture.vfs().await;
    conformance::write_file(
        &vfs,
        Path::new("/a.txt"),
        rc_vfs::WriteMode::CreateNew,
        b"abc",
    )
    .await;
    assert_eq!(std::fs::read(fixture.local("a.txt")).expect("read"), b"abc");
    vfs.set_attributes(
        Path::new("/a.txt"),
        rc_vfs::SetAttributes {
            mode: Some(0o600),
            readonly: None,
        },
    )
    .await
    .expect("chmod");
    assert_eq!(
        vfs.stat(Path::new("/a.txt"))
            .await
            .expect("stat")
            .attributes
            .mode,
        Some(0o600)
    );
    vfs.set_attributes(
        Path::new("/a.txt"),
        rc_vfs::SetAttributes {
            mode: None,
            readonly: Some(true),
        },
    )
    .await
    .expect("read-only");
    let entry = vfs.stat(Path::new("/a.txt")).await.expect("stat");
    assert_eq!(entry.attributes.mode, Some(0o400));
    assert!(entry.attributes.readonly);
}
