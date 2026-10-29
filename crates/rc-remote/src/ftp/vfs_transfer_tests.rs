//! Data connections against the in-process test server: active mode (`PORT`/`EPRT` to a
//! listener on the control connection's address), uploads to a server that sends TLS 1.3
//! session tickets, data connections that must resume the control connection's TLS session,
//! and a download and an upload the server stalls.

use std::{
    io,
    path::Path,
    time::{Duration, Instant},
};

use rc_vfs::{Vfs, WriteMode, conformance};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use super::{FtpVfs, tests::config};
use crate::ftp::{
    DataMode, Encryption,
    test_server::{Options, ServerTls, TestServer},
};

async fn server(options: Options) -> TestServer {
    TestServer::start(options).await
}

async fn active_conformance(tls: ServerTls, encryption: Encryption) {
    let server = server(Options {
        tls,
        ..Options::default()
    })
    .await;
    std::fs::create_dir(server.local("conformance")).expect("root");
    let mut config = config(&server, encryption);
    config.data_mode = DataMode::Active;
    let vfs = FtpVfs::connect(config).await.expect("connect");
    conformance::run(&vfs, Path::new("/conformance")).await;
}

#[tokio::test]
async fn active_mode_conforms() {
    active_conformance(ServerTls::None, Encryption::None).await;
}

#[tokio::test]
async fn active_mode_over_explicit_ftps_conforms() {
    active_conformance(ServerTls::Explicit, Encryption::Explicit).await;
}

/// The bug the integration of wave 010-w2 found: the tickets an upload never read turned the
/// close into a reset, and the server lost the upload's tail.
async fn upload_with_tickets(tls: ServerTls, encryption: Encryption, data_mode: DataMode) {
    let server = server(Options {
        tls,
        tickets: true,
        ..Options::default()
    })
    .await;
    let mut config = config(&server, encryption);
    config.data_mode = data_mode;
    let vfs = FtpVfs::connect(config).await.expect("connect");
    let content: Vec<u8> = (0..1024 * 1024)
        .map(|index| u8::try_from(index % 251).expect("below 251"))
        .collect();
    let mut upload = vfs
        .open_write(Path::new("/big.bin"), WriteMode::CreateNew)
        .await
        .expect("open");
    upload.write_all(&content).await.expect("write");
    upload.shutdown().await.expect("the server confirmed it");
    drop(upload);
    let stored = std::fs::read(server.local("big.bin")).expect("stored");
    assert_eq!(stored.len(), content.len(), "the whole upload arrived");
    assert!(stored == content, "byte for byte");
}

#[tokio::test]
async fn an_upload_survives_tls13_tickets_explicit() {
    upload_with_tickets(ServerTls::Explicit, Encryption::Explicit, DataMode::Passive).await;
}

#[tokio::test]
async fn an_upload_survives_tls13_tickets_implicit() {
    upload_with_tickets(ServerTls::Implicit, Encryption::Implicit, DataMode::Passive).await;
}

#[tokio::test]
async fn an_active_upload_survives_tls13_tickets() {
    upload_with_tickets(ServerTls::Explicit, Encryption::Explicit, DataMode::Active).await;
}

#[tokio::test]
async fn a_stalled_download_ends_after_the_idle_limit() {
    let server = server(Options {
        stall: true,
        ..Options::default()
    })
    .await;
    std::fs::write(server.local("slow.bin"), vec![1; 64 * 1024]).expect("file");
    let mut config = config(&server, Encryption::None);
    config.timeout = Duration::from_secs(1);
    let vfs = FtpVfs::connect(config).await.expect("connect");
    let mut download = vfs.open_read(Path::new("/slow.bin")).await.expect("open");
    let started = Instant::now();
    let mut received = Vec::new();
    // Four bytes come, then nothing: the read fails once the idle limit passes.
    let error = download
        .read_to_end(&mut received)
        .await
        .expect_err("stalled");
    assert_eq!(error.kind(), io::ErrorKind::TimedOut, "{error}");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn a_stalled_upload_ends_after_the_idle_limit() {
    let server = server(Options {
        stall: true,
        ..Options::default()
    })
    .await;
    let mut config = config(&server, Encryption::None);
    config.timeout = Duration::from_secs(1);
    let vfs = FtpVfs::connect(config).await.expect("connect");
    let mut upload = vfs
        .open_write(Path::new("/slow.bin"), WriteMode::CreateNew)
        .await
        .expect("open");
    let started = Instant::now();
    // The server reads four bytes, then nothing: once the socket buffers are full, the write
    // that cannot go on fails at the idle limit. 256 MiB is far more than any buffer holds.
    let chunk = vec![0_u8; 1024 * 1024];
    let mut failure = None;
    for _ in 0..256 {
        if let Err(error) = upload.write_all(&chunk).await {
            failure = Some(error);
            break;
        }
    }
    let error = failure.expect("the upload stalled");
    assert_eq!(error.kind(), io::ErrorKind::TimedOut, "{error}");
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "{:?}",
        started.elapsed()
    );
}

/// vsftpd (`require_ssl_reuse`) and FileZilla Server refuse a TLS data connection that does not
/// resume a session; the test server does too with `require_reuse`. Passive and active.
async fn transfers_resuming_the_session(
    tls: ServerTls,
    encryption: Encryption,
    data_mode: DataMode,
) {
    let server = server(Options {
        tls,
        require_reuse: true,
        ..Options::default()
    })
    .await;
    std::fs::write(server.local("in.txt"), b"resumed").expect("file");
    let mut config = config(&server, encryption);
    config.data_mode = data_mode;
    let vfs = FtpVfs::connect(config).await.expect("connect");
    let listed = vfs
        .list(Path::new("/"))
        .await
        .expect("a listing over a resumed session");
    assert!(listed.iter().any(|entry| entry.name == "in.txt"));
    let mut content = Vec::new();
    vfs.open_read(Path::new("/in.txt"))
        .await
        .expect("open")
        .read_to_end(&mut content)
        .await
        .expect("a download over a resumed session");
    assert_eq!(content, b"resumed");
    let mut upload = vfs
        .open_write(Path::new("/out.txt"), WriteMode::CreateNew)
        .await
        .expect("open");
    upload.write_all(b"also resumed").await.expect("write");
    upload
        .shutdown()
        .await
        .expect("an upload over a resumed session");
    drop(upload);
    assert_eq!(
        std::fs::read(server.local("out.txt")).expect("stored"),
        b"also resumed"
    );
}

#[tokio::test]
async fn passive_data_connections_resume_the_tls_session() {
    transfers_resuming_the_session(ServerTls::Explicit, Encryption::Explicit, DataMode::Passive)
        .await;
    transfers_resuming_the_session(ServerTls::Implicit, Encryption::Implicit, DataMode::Passive)
        .await;
}

#[tokio::test]
async fn active_data_connections_resume_the_tls_session() {
    transfers_resuming_the_session(ServerTls::Explicit, Encryption::Explicit, DataMode::Active)
        .await;
}

/// Two control connections at once, each with a download open: each data connection has to
/// resume the session of its own control connection, which the test server checks - a ticket
/// of the other one would be a full handshake there, and refused.
#[tokio::test]
async fn parallel_control_connections_resume_only_their_own_sessions() {
    for (tls, encryption, data_mode) in [
        (ServerTls::Explicit, Encryption::Explicit, DataMode::Passive),
        (ServerTls::Implicit, Encryption::Implicit, DataMode::Passive),
        (ServerTls::Explicit, Encryption::Explicit, DataMode::Active),
    ] {
        let server = server(Options {
            tls,
            require_reuse: true,
            ..Options::default()
        })
        .await;
        std::fs::write(server.local("a.txt"), b"first").expect("file");
        std::fs::write(server.local("b.txt"), b"second").expect("file");
        let mut config = config(&server, encryption);
        config.data_mode = data_mode;
        let vfs = FtpVfs::connect(config).await.expect("connect");
        for _ in 0..3 {
            // The first lease takes the idle control connection, the second opens another.
            let mut first = vfs.open_read(Path::new("/a.txt")).await.expect("first");
            let mut second = vfs.open_read(Path::new("/b.txt")).await.expect("second");
            let (mut a, mut b) = (Vec::new(), Vec::new());
            second
                .read_to_end(&mut b)
                .await
                .expect("second over its own session");
            first
                .read_to_end(&mut a)
                .await
                .expect("first over its own session");
            assert_eq!(
                (a.as_slice(), b.as_slice()),
                (&b"first"[..], &b"second"[..])
            );
        }
    }
}

/// RC-070-03: two downloads at once run on two connections, a third waits for one of them, and
/// the listing goes on beside them on a connection of its own.
#[tokio::test]
async fn the_listing_stays_usable_while_the_transfers_are_at_their_limit() {
    let server = server(Options::default()).await;
    for name in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(server.local(name), name).expect("file");
    }
    let vfs = FtpVfs::connect(config(&server, Encryption::None))
        .await
        .expect("connect");
    let mut first = vfs.open_read(Path::new("/a.txt")).await.expect("first");
    let second = vfs.open_read(Path::new("/b.txt")).await.expect("second");
    assert_eq!(server.passwords_received(), 2, "two transfers, two logins");
    let third = tokio::time::timeout(
        Duration::from_millis(300),
        vfs.open_read(Path::new("/c.txt")),
    )
    .await;
    assert!(third.is_err(), "the third download waits");

    let listed = tokio::time::timeout(Duration::from_secs(5), vfs.list(Path::new("/")))
        .await
        .expect("the listing does not wait for the downloads")
        .expect("listed");
    assert_eq!(listed.len(), 3);

    let mut content = Vec::new();
    first.read_to_end(&mut content).await.expect("first read");
    assert_eq!(content, b"a.txt");
    drop(first);
    let mut third =
        tokio::time::timeout(Duration::from_secs(5), vfs.open_read(Path::new("/c.txt")))
            .await
            .expect("a finished download frees its place")
            .expect("third");
    content.clear();
    third.read_to_end(&mut content).await.expect("third read");
    assert_eq!(content, b"c.txt");
    assert_eq!(
        server.passwords_received(),
        3,
        "no login beyond the limit plus one"
    );
    drop(second);
}
