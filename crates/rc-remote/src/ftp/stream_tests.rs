use std::{io, sync::Arc, time::Duration};

use secrecy::SecretString;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{Download, State, Upload, reply_kind};
use crate::ftp::{
    Credentials, Encryption, FtpConfig,
    pool::{Pool, Use},
    test_server::{Options, PASSWORD, TestServer},
};

#[test]
fn a_transfer_the_connection_ended_is_aborted_and_a_refusal_is_not() {
    for code in [421, 425, 426] {
        assert_eq!(reply_kind(code), io::ErrorKind::ConnectionAborted, "{code}");
    }
    for code in [450, 451, 452, 550, 552] {
        assert_eq!(reply_kind(code), io::ErrorKind::Other, "{code}");
    }
}

/// A server that sends every `226` only after five seconds, and a pool on it.
async fn slow_to_confirm() -> (TestServer, Arc<Pool>) {
    let server = TestServer::start(Options {
        hold: Some(("226", Duration::from_secs(5))),
        ..Options::default()
    })
    .await;
    std::fs::write(server.local("file"), b"content").expect("a file to read");
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
    let pool = Pool::open(config).await.expect("open");
    (server, pool)
}

/// The connection the dropped stream held is not reused: the next call logs in anew.
async fn the_connection_was_closed(server: &TestServer, pool: &Arc<Pool>) {
    drop(pool.lease(Use::Command).await.expect("next"));
    assert_eq!(
        server.passwords_received(),
        2,
        "the connection owed a 226 is closed"
    );
}

#[tokio::test]
async fn a_download_dropped_while_it_waits_for_226_closes_its_connection() {
    let (server, pool) = slow_to_confirm().await;
    let mut lease = pool.lease(Use::Transfer).await.expect("lease");
    let transfer = lease.retr("/file").await.expect("RETR");
    let mut download = Download::new(transfer, lease);
    let mut bytes = Vec::new();
    let read =
        tokio::time::timeout(Duration::from_millis(500), download.read_to_end(&mut bytes)).await;
    assert!(read.is_err(), "the server holds its 226 back");
    assert!(
        matches!(download.state, State::Finishing(_)),
        "the data is through"
    );
    drop(download);
    the_connection_was_closed(&server, &pool).await;
}

#[tokio::test]
async fn an_upload_dropped_while_it_waits_for_226_closes_its_connection() {
    let (server, pool) = slow_to_confirm().await;
    let mut lease = pool.lease(Use::Transfer).await.expect("lease");
    let transfer = lease.stor("/up").await.expect("STOR");
    let mut upload = Upload::new(transfer, lease);
    upload.write_all(b"data").await.expect("write");
    let shutdown = tokio::time::timeout(Duration::from_millis(500), upload.shutdown()).await;
    assert!(shutdown.is_err(), "the server holds its 226 back");
    assert!(
        matches!(upload.state, State::Finishing(_)),
        "the data is through"
    );
    drop(upload);
    the_connection_was_closed(&server, &pool).await;
}
