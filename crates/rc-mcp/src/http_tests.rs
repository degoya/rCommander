use std::{
    net::Ipv4Addr,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Router,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    routing::get,
};
use tokio::{io::AsyncWriteExt as _, net::TcpListener, sync::Notify};
use tokio_util::sync::CancellationToken;

use super::{HttpHandle, check, is_loopback_authority, is_loopback_origin, spawn_server};
use crate::McpToken;

fn headers(pairs: &[(header::HeaderName, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.insert(name, HeaderValue::from_str(value).expect("header"));
    }
    map
}

#[test]
fn loopback_hosts_and_origins_pass_and_others_do_not() {
    for host in ["127.0.0.1:8080", "localhost", "LOCALHOST:1", "[::1]:9"] {
        assert!(is_loopback_authority(host), "{host}");
    }
    for host in [
        "evil.example",
        "evil.example:80",
        "127.0.0.1.evil.example",
        "",
        "10.0.0.1",
    ] {
        assert!(!is_loopback_authority(host), "{host}");
    }
    for origin in [
        "http://localhost:3000",
        "https://127.0.0.1",
        "http://[::1]:1",
    ] {
        assert!(is_loopback_origin(origin), "{origin}");
    }
    for origin in [
        "null",
        "http://evil.example",
        "file:///etc",
        "http://localhost.evil.example",
        "chrome-extension://abc",
    ] {
        assert!(!is_loopback_origin(origin), "{origin}");
    }
}

#[test]
fn the_guard_wants_host_origin_and_token() {
    let token = McpToken::from_text("t0k3n");
    let good = [
        (header::HOST, "127.0.0.1:5"),
        (header::AUTHORIZATION, "Bearer t0k3n"),
    ];
    assert!(check(&token, &headers(&good)).is_ok());
    let lower = [
        (header::HOST, "localhost:5"),
        (header::AUTHORIZATION, "bearer t0k3n"),
        (header::ORIGIN, "http://localhost:5"),
    ];
    assert!(check(&token, &headers(&lower)).is_ok());

    let status = |pairs: &[(header::HeaderName, &str)]| {
        check(&token, &headers(pairs)).expect_err("refused").status
    };
    assert_eq!(
        status(&[(header::HOST, "127.0.0.1:5")]),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        status(&[
            (header::HOST, "127.0.0.1:5"),
            (header::AUTHORIZATION, "Bearer wrong")
        ]),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        status(&[
            (header::HOST, "127.0.0.1:5"),
            (header::AUTHORIZATION, "Basic t0k3n")
        ]),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        status(&[
            (header::HOST, "rebound.example:5"),
            (header::AUTHORIZATION, "Bearer t0k3n")
        ]),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        status(&[
            (header::HOST, "127.0.0.1:5"),
            (header::AUTHORIZATION, "Bearer t0k3n"),
            (header::ORIGIN, "http://evil.example")
        ]),
        StatusCode::FORBIDDEN
    );
}

/// A request that never ends does not hold the shutdown past its grace (RC-080-58).
#[tokio::test]
async fn the_shutdown_ends_with_a_request_still_open() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("a port");
    let addr = listener.local_addr().expect("the address");
    let entered = Arc::new(Notify::new());
    let reached = Arc::clone(&entered);
    let app = Router::new().route(
        "/hang",
        get(move || {
            reached.notify_one();
            std::future::pending::<&'static str>()
        }),
    );
    let stop = CancellationToken::new();
    let handle = HttpHandle {
        addr,
        stop: stop.clone(),
        task: Some(spawn_server(listener, app, stop)),
        instance_file: None,
    };
    let mut client = tokio::net::TcpStream::connect(addr)
        .await
        .expect("connects");
    client
        .write_all(b"GET /hang HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .await
        .expect("the request is sent");
    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("the request reached its handler");

    let start = Instant::now();
    let stopped = tokio::time::timeout(
        Duration::from_secs(5),
        handle.shutdown_within(Duration::from_millis(200)),
    )
    .await
    .expect("the shutdown ends after its grace");
    assert!(stopped.is_ok(), "{stopped:?}");
    let took = start.elapsed();
    assert!(took >= Duration::from_millis(200), "{took:?}");
    // The client keeps its side open until here.
    drop(client);
}
