use std::time::Duration;

use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpListener,
};

use super::HttpGrant;
use crate::failure::codes;

/// A server on a free loopback port that answers one connection with `answer`, and the port.
async fn serve_once(answer: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("address").port();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream.read(&mut buffer).await.expect("read");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
        }
        stream.write_all(answer.as_bytes()).await.expect("write");
    });
    port
}

fn grant(port: u16) -> HttpGrant {
    HttpGrant::new(port, 1024, Duration::from_secs(5)).expect("a client")
}

fn denied(grant: &HttpGrant, url: &str) -> bool {
    grant
        .target(url)
        .is_err_and(|failure| failure.code() == Some(codes::PLUGIN_HTTP_DENIED))
}

/// Only plain HTTP to this machine's loopback on the granted port, without a user.
#[test]
fn only_the_granted_loopback_port_is_a_target() {
    let grant = grant(8087);
    for allowed in [
        "http://localhost:8087/status",
        "http://LOCALHOST:8087/list/object/items?search=srv",
        "http://127.0.0.1:8087/",
        "http://[::1]:8087/",
    ] {
        assert!(grant.target(allowed).is_ok(), "{allowed}");
    }
    for refused in [
        "http://localhost:8088/",
        "http://localhost/",
        "https://localhost:8087/",
        "http://127.0.0.2:8087/",
        "http://10.0.0.1:8087/",
        "http://example.net:8087/",
        "http://user:pass@localhost:8087/",
        "file:///etc/passwd",
        "not a url",
    ] {
        assert!(denied(&grant, refused), "{refused}");
    }
}

#[test]
fn a_growing_answer_moves_into_a_larger_buffer() {
    let mut body = zeroize::Zeroizing::new(Vec::new());
    for chunk in [&b"abc"[..], b"defgh", b"", b"ijklmnopqrstuvwxyz"] {
        super::append(&mut body, chunk);
    }
    assert_eq!(body.as_slice(), b"abcdefghijklmnopqrstuvwxyz");
}

#[tokio::test]
async fn an_answer_comes_back_whole() {
    let port = serve_once(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}",
    )
    .await;
    let answer = grant(port)
        .request(
            "GET",
            &format!("http://localhost:{port}/status"),
            &[],
            vec![],
        )
        .await
        .expect("an answer");
    assert_eq!(answer.status, 200);
    assert_eq!(answer.body, b"{}");
    assert!(
        answer
            .headers
            .iter()
            .any(|(name, value)| name == "content-type" && value == "application/json")
    );
}

/// A redirect is the answer: following it could leave the granted port.
#[tokio::test]
async fn a_redirect_is_answered_not_followed() {
    let port = serve_once(
        "HTTP/1.1 302 Found\r\nLocation: http://example.net/\r\nContent-Length: 0\r\n\r\n",
    )
    .await;
    let answer = grant(port)
        .request("GET", &format!("http://127.0.0.1:{port}/"), &[], vec![])
        .await
        .expect("an answer");
    assert_eq!(answer.status, 302);
}

#[tokio::test]
async fn an_answer_over_the_limit_is_refused() {
    let port = serve_once("HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\n{\"a\":12}").await;
    let small = HttpGrant::new(port, 4, Duration::from_secs(5)).expect("a client");
    let failure = small
        .request("GET", &format!("http://localhost:{port}/"), &[], vec![])
        .await
        .expect_err("refused");
    assert_eq!(failure.code(), Some(codes::PLUGIN_HTTP_TOO_LARGE));
}

#[tokio::test]
async fn nothing_listening_is_a_failed_request() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("address").port();
    drop(listener);
    let failure = grant(port)
        .request("GET", &format!("http://localhost:{port}/"), &[], vec![])
        .await
        .expect_err("refused");
    assert_eq!(failure.code(), Some(codes::PLUGIN_HTTP_FAILED));
}

/// A method outside the list and a header the host sets itself are refused before anything is
/// sent.
#[tokio::test]
async fn a_strange_method_or_a_reserved_header_is_refused() {
    let grant = grant(9);
    let url = "http://localhost:9/";
    let refused = grant.request("CONNECT", url, &[], vec![]).await;
    assert_eq!(
        refused.expect_err("refused").code(),
        Some(codes::PLUGIN_BAD_OUTPUT)
    );
    let host = [("Host".to_owned(), "example.net".to_owned())];
    let refused = grant.request("GET", url, &host, vec![]).await;
    assert_eq!(
        refused.expect_err("refused").code(),
        Some(codes::PLUGIN_BAD_OUTPUT)
    );
    let refused = grant
        .request("POST", url, &[], vec![0; 2 * 1024 * 1024])
        .await;
    assert_eq!(
        refused.expect_err("refused").code(),
        Some(codes::PLUGIN_REQUEST_TOO_LARGE)
    );
}
