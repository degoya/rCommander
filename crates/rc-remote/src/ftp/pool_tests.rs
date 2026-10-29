use secrecy::SecretString;

use super::*;
use crate::ftp::{
    Credentials,
    test_server::{Options, PASSWORD, TestServer},
};

#[tokio::test]
async fn a_listing_within_the_limit_is_read_whole() {
    let bytes = read_capped(&b"type=file; a\r\n"[..], 14)
        .await
        .expect("within the limit");
    assert_eq!(bytes, b"type=file; a\r\n");
}

#[tokio::test]
async fn a_listing_past_the_limit_is_a_protocol_failure() {
    // An endless listing: only `limit + 1` bytes are ever read.
    let error = read_capped(tokio::io::repeat(b'x'), 1024)
        .await
        .expect_err("too large");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    let inner = error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<FtpError>())
        .expect("an FtpError inside");
    assert_eq!(inner.code(), "ftp_protocol");
}

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

/// `now` minus `age`; the test machine's clock reaches back that far.
fn ago(age: Duration) -> Instant {
    Instant::now()
        .checked_sub(age)
        .expect("the monotonic clock reaches back far enough")
}

#[tokio::test]
async fn the_home_is_where_the_login_started() {
    let server = TestServer::start(Options {
        start: "/srv/www",
        ..Options::default()
    })
    .await;
    let pool = Pool::open(config(&server)).await.expect("open");
    assert_eq!(pool.home, PathBuf::from("/srv/www"));
}

#[tokio::test]
async fn a_pwd_that_timed_out_leaves_no_connection_in_the_pool() {
    // `257 "/"` is the `PWD` of the root only; `MKD` answers `257 "/name" created`.
    let server = TestServer::start(Options {
        hold: Some(("257 \"/\"", Duration::from_secs(5))),
        ..Options::default()
    })
    .await;
    let mut config = config(&server);
    config.timeout = Duration::from_secs(1);
    let pool = Pool::open(config).await.expect("open without a home");
    assert_eq!(pool.home, PathBuf::from("/"));
    assert!(
        pool.idle().is_empty(),
        "the connection owed a 257 is closed"
    );
    drop(pool.lease(Use::Command).await.expect("next"));
    assert_eq!(server.passwords_received(), 2, "a new login");
}

#[test]
fn an_odd_pwd_answer_is_not_followed() {
    assert_eq!(home_of("/home/web"), PathBuf::from("/home/web"));
    assert_eq!(home_of("home/web"), PathBuf::from("/"));
    assert_eq!(home_of("C:/inetpub"), PathBuf::from("/"));
    assert_eq!(home_of("/a\r\nDELE b"), PathBuf::from("/"));
    assert_eq!(home_of(""), PathBuf::from("/"));
}

#[test]
fn a_pwd_answer_takes_the_rules_of_an_sftp_home() {
    assert_eq!(home_of("//srv//www/"), PathBuf::from("/srv/www"));
    // Each of these would make every later path a refusal (`ftp_path`).
    for hostile in ["/srv/../etc", "/srv/..", "/srv/./www", "/srv\\www", "/a\0b"] {
        assert_eq!(home_of(hostile), PathBuf::from("/"), "{hostile:?}");
    }
}

/// A server that answers a reply starting with `reply` only after five seconds, and a pool on it.
async fn holding(reply: &'static str) -> (TestServer, Arc<Pool>) {
    let server = TestServer::start(Options {
        hold: Some((reply, Duration::from_secs(5))),
        ..Options::default()
    })
    .await;
    std::fs::write(server.local("file"), b"content").expect("a file to read");
    let pool = Pool::open(config(&server)).await.expect("open");
    (server, pool)
}

/// Drops `call` half-way, as a cancelled panel load does (`select!` on the cancel token).
async fn give_up<F: Future>(call: F) {
    let call = tokio::time::timeout(Duration::from_millis(500), call).await;
    assert!(call.is_err(), "the server still holds its reply back");
}

/// After a call was dropped half-way, its connection is closed, not reused: the next command
/// logs in anew and reads its own reply, not the one left unread.
async fn the_next_command_reads_its_own_reply(server: &TestServer, pool: &Arc<Pool>) {
    assert!(
        pool.idle().is_empty(),
        "the connection owed a reply is closed"
    );
    let mut next = pool.lease(Use::Command).await.expect("next");
    next.mkdir("/fresh").await.expect("MKD reads its own 257");
    assert!(server.local("fresh").is_dir());
    assert_eq!(server.passwords_received(), 2, "a new login");
}

#[tokio::test]
async fn a_cwd_dropped_half_way_leaves_no_reply_for_the_next_command() {
    let (server, pool) = holding("250 Directory").await;
    let mut lease = pool.lease(Use::Command).await.expect("lease");
    give_up(lease.cwd("/")).await;
    drop(lease);
    the_next_command_reads_its_own_reply(&server, &pool).await;
}

#[tokio::test]
async fn a_listing_dropped_before_its_226_leaves_no_reply_for_the_next_command() {
    let (server, pool) = holding("226").await;
    let mut lease = pool.lease(Use::Command).await.expect("lease");
    give_up(lease.listing("MLSD")).await;
    drop(lease);
    the_next_command_reads_its_own_reply(&server, &pool).await;
}

#[tokio::test]
async fn a_retr_dropped_before_its_150_leaves_no_reply_for_the_next_command() {
    let (server, pool) = holding("150").await;
    let mut lease = pool.lease(Use::Transfer).await.expect("lease");
    give_up(lease.retr("/file")).await;
    drop(lease);
    the_next_command_reads_its_own_reply(&server, &pool).await;
}

#[tokio::test]
async fn a_command_that_was_answered_goes_back_to_the_pool() {
    let (server, pool) = holding("226").await;
    let mut lease = pool.lease(Use::Command).await.expect("lease");
    lease.cwd("/").await.expect("CWD");
    assert!(
        lease.size("/missing").await.is_err(),
        "a refusal is an answer"
    );
    drop(lease);
    assert_eq!(pool.idle().len(), 1, "a connection owed nothing is kept");
    drop(pool.lease(Use::Command).await.expect("again"));
    assert_eq!(server.passwords_received(), 1, "and reused");
}

#[tokio::test]
async fn transfers_wait_past_the_limit_while_commands_go_on() {
    let server = TestServer::start(Options::default()).await;
    let pool = Pool::open(config(&server)).await.expect("open");
    assert_eq!(server.passwords_received(), 1);
    let first = pool.lease(Use::Transfer).await.expect("first");
    let second = pool.lease(Use::Transfer).await.expect("second");
    assert_eq!(server.passwords_received(), 2, "two transfers, two logins");
    let third = tokio::time::timeout(Duration::from_millis(300), pool.lease(Use::Transfer)).await;
    assert!(third.is_err(), "a third transfer waits for a free one");

    let mut command = tokio::time::timeout(Duration::from_secs(5), pool.lease(Use::Command))
        .await
        .expect("a command does not wait for the transfers")
        .expect("a command connection");
    command.cwd("/").await.expect("the command runs");
    drop(command);
    assert_eq!(server.passwords_received(), 3, "commands have their own");

    drop(first);
    let third = tokio::time::timeout(Duration::from_secs(5), pool.lease(Use::Transfer))
        .await
        .expect("free once one ended")
        .expect("third");
    assert_eq!(
        server.passwords_received(),
        3,
        "an idle connection is reused"
    );
    drop((second, third));
}

#[tokio::test]
async fn one_command_at_a_time() {
    let server = TestServer::start(Options::default()).await;
    let pool = Pool::open(config(&server)).await.expect("open");
    let first = pool.lease(Use::Command).await.expect("first");
    let second = tokio::time::timeout(Duration::from_millis(300), pool.lease(Use::Command)).await;
    assert!(second.is_err(), "the listing connection is one");
    drop(first);
    assert_eq!(server.passwords_received(), 1);
}

#[tokio::test]
async fn a_connection_idle_too_long_is_closed_not_reused() {
    let server = TestServer::start(Options::default()).await;
    let pool = Pool::open(config(&server)).await.expect("open");
    pool.idle()[0].used = ago(IDLE_TIMEOUT);
    drop(pool.lease(Use::Command).await.expect("lease"));
    assert_eq!(server.passwords_received(), 2, "a new login");
}

#[tokio::test]
async fn the_keepalive_closes_the_old_and_keeps_the_rest() {
    let server = TestServer::start(Options::default()).await;
    let pool = Pool::open(config(&server)).await.expect("open");
    // Two at once: two connections, both idle afterwards.
    let command = pool.lease(Use::Command).await.expect("command");
    let transfer = pool.lease(Use::Transfer).await.expect("transfer");
    drop((command, transfer));
    assert_eq!(pool.idle().len(), 2);
    {
        let mut idle = pool.idle();
        idle[0].used = ago(IDLE_TIMEOUT);
        idle[1].checked = ago(STALE_AFTER);
    }
    pool.refresh().await;
    let idle = pool.idle();
    assert_eq!(idle.len(), 1, "the one unused too long is closed");
    assert!(
        idle[0].checked.elapsed() < STALE_AFTER,
        "the other answered NOOP"
    );
}

#[tokio::test]
async fn the_keepalive_ends_with_the_pool() {
    let server = TestServer::start(Options::default()).await;
    let pool = Pool::open(config(&server)).await.expect("open");
    let weak = Arc::downgrade(&pool);
    drop(pool);
    assert!(
        weak.upgrade().is_none(),
        "the task holds no strong reference"
    );
}

/// Waits until `server` has `count` control connections open, for at most five seconds.
async fn sessions_reach(server: &TestServer, count: usize) {
    for _ in 0..500 {
        if server.open_sessions() == count {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!(
        "{} control connections open, not {count}",
        server.open_sessions()
    );
}

#[tokio::test]
async fn close_ends_every_idle_connection_and_keeps_none_after() {
    let server = TestServer::start(Options::default()).await;
    let pool = Pool::open(config(&server)).await.expect("open");
    let command = pool.lease(Use::Command).await.expect("command");
    let transfer = pool.lease(Use::Transfer).await.expect("transfer");
    drop((command, transfer));
    assert_eq!(pool.idle().len(), 2);
    sessions_reach(&server, 2).await;

    pool.close().await;
    assert!(pool.idle().is_empty());
    sessions_reach(&server, 0).await;

    // A holder of the closed pool still gets its call done, on a connection of its own.
    let late = pool.lease(Use::Command).await.expect("a login of its own");
    drop(late);
    assert!(pool.idle().is_empty(), "nothing is kept after the close");
    sessions_reach(&server, 0).await;
}
