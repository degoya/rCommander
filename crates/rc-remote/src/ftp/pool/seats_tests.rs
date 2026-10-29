use std::{path::Path, time::Duration};

use rc_vfs::Vfs;
use secrecy::SecretString;
use tokio::io::AsyncReadExt;

use crate::ftp::{
    Credentials, Encryption, FtpConfig, FtpError, FtpVfs,
    test_server::{Options, PASSWORD, TestServer},
};

fn config(server: &TestServer) -> FtpConfig {
    let mut config = FtpConfig::new(
        "127.0.0.1",
        Encryption::None,
        Credentials::User {
            name: "tester".to_owned(),
            password: SecretString::from(PASSWORD),
        },
    );
    config.port = server.port;
    config.timeout = Duration::from_secs(10);
    config
}

async fn server(max_sessions: usize) -> TestServer {
    let server = TestServer::start(Options {
        max_sessions,
        ..Options::default()
    })
    .await;
    for name in ["a.bin", "b.bin"] {
        std::fs::write(server.local(name), vec![7_u8; 64 * 1024]).expect("file");
    }
    server
}

#[tokio::test]
async fn an_idle_connection_the_server_dropped_is_replaced_before_use() {
    let server = server(0).await;
    let vfs = FtpVfs::connect(config(&server)).await.expect("connect");
    vfs.list(Path::new("/")).await.expect("listed");
    server.drop_sessions();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let listed = vfs
        .list(Path::new("/"))
        .await
        .expect("listed after the drop");
    assert!(listed.iter().any(|entry| entry.name == "a.bin"));
    assert_eq!(
        server.passwords_received(),
        2,
        "a new login, no failed call"
    );
}

#[tokio::test]
async fn a_server_with_too_few_sessions_makes_the_call_wait_for_one() {
    // Two sessions: one per running download, none left for the panel's listing.
    let server = server(2).await;
    let vfs = FtpVfs::connect(config(&server)).await.expect("connect");
    let mut first = vfs
        .open_read(Path::new("/a.bin"))
        .await
        .expect("first download");
    let second = vfs
        .open_read(Path::new("/b.bin"))
        .await
        .expect("second download");

    let listing = tokio::spawn({
        let vfs = vfs.clone();
        async move { vfs.list(Path::new("/")).await }
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        !listing.is_finished(),
        "the listing waits, it does not fail"
    );

    let mut bytes = Vec::new();
    first.read_to_end(&mut bytes).await.expect("read");
    drop(first);
    let listed = tokio::time::timeout(Duration::from_secs(10), listing)
        .await
        .expect("listed once a download ended")
        .expect("joined")
        .expect("listed");
    assert!(listed.iter().any(|entry| entry.name == "b.bin"));
    drop(second);
    // The pool keeps to two sessions from now on: a third download waits instead of failing.
    let third = vfs
        .open_read(Path::new("/a.bin"))
        .await
        .expect("third download");
    drop(third);
}

#[tokio::test]
async fn a_server_that_takes_no_session_at_all_names_it() {
    let server = server(1).await;
    let _first = FtpVfs::connect(config(&server))
        .await
        .expect("the one session");
    let refused = FtpVfs::connect(config(&server))
        .await
        .expect_err("no second session");
    assert!(
        matches!(refused, FtpError::TooManySessions(_)),
        "{refused:?}"
    );
    assert_eq!(refused.code(), "ftp_too_many_sessions");
}

#[tokio::test]
async fn a_session_the_server_has_not_let_go_yet_does_not_shrink_the_pool() {
    // pure-ftpd counts a closed session for a moment longer: the login right after a
    // cancelled download is refused, though the server took two sessions before.
    let server = TestServer::start(Options {
        max_sessions: 2,
        linger: Duration::from_millis(300),
        ..Options::default()
    })
    .await;
    for name in ["a.bin", "b.bin"] {
        std::fs::write(server.local(name), vec![7_u8; 64 * 1024]).expect("file");
    }
    let vfs = FtpVfs::connect(config(&server)).await.expect("connect");
    let first = vfs.open_read(Path::new("/a.bin")).await.expect("first");
    let second = vfs.open_read(Path::new("/b.bin")).await.expect("second");
    drop(second);
    let third = tokio::time::timeout(Duration::from_secs(5), vfs.open_read(Path::new("/b.bin")))
        .await
        .expect("starts once the server let go of the cancelled one")
        .expect("third");
    drop((first, third));
    // Still two transfers at once: the refusals did not cost the pool a seat.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let both = tokio::time::timeout(Duration::from_secs(5), async {
        let one = vfs.open_read(Path::new("/a.bin")).await.expect("one");
        let two = vfs.open_read(Path::new("/b.bin")).await.expect("two");
        (one, two)
    })
    .await;
    assert!(both.is_ok(), "two downloads at once again");
}
