use std::net::{Ipv4Addr, Ipv6Addr};

use super::*;

const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

#[test]
fn the_address_goes_out_as_port_or_eprt() {
    let v4 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)), 5001);
    assert_eq!(port_command(v4), "PORT 192,168,1,20,19,137");
    let v6 = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 2121);
    assert_eq!(port_command(v6), "EPRT |2|::1|2121|");
}

#[test]
fn only_the_control_peer_is_the_server() {
    let mapped = IpAddr::V6(Ipv4Addr::LOCALHOST.to_ipv6_mapped());
    assert!(is_server(LOOPBACK, LOOPBACK));
    assert!(is_server(mapped, LOOPBACK));
    assert!(is_server(LOOPBACK, mapped));
    assert!(!is_server(
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)),
        LOOPBACK
    ));
}

#[tokio::test]
async fn the_listener_binds_the_control_address_only() {
    let listener = listen(LOOPBACK).await.expect("listen");
    let address = listener.local_addr().expect("address");
    assert_eq!(address.ip(), LOOPBACK, "never every interface");
    assert_ne!(address.port(), 0);
}

#[tokio::test]
async fn the_server_s_connection_is_taken() {
    let listener = listen(LOOPBACK).await.expect("listen");
    let address = listener.local_addr().expect("address");
    let client = tokio::spawn(TcpStream::connect(address));
    let socket = accept(&listener, LOOPBACK, Duration::from_secs(5))
        .await
        .expect("from the server");
    assert_eq!(socket.peer_addr().expect("peer").ip(), LOOPBACK);
    client.await.expect("join").expect("connected");
}

#[tokio::test]
async fn a_connection_from_anyone_else_fails_the_transfer() {
    let listener = listen(LOOPBACK).await.expect("listen");
    let address = listener.local_addr().expect("address");
    let _intruder = tokio::spawn(TcpStream::connect(address));
    // The control connection's server is another address than the one that connected.
    let server = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));
    let error = accept(&listener, server, Duration::from_secs(5))
        .await
        .expect_err("refused");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert!(error.to_string().contains("127.0.0.1"), "{error}");
}

#[tokio::test]
async fn no_connection_in_time_fails_the_transfer() {
    let listener = listen(LOOPBACK).await.expect("listen");
    let error = accept(&listener, LOOPBACK, Duration::from_millis(50))
        .await
        .expect_err("nobody came");
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
}

/// What the server sent and the client never read (TLS 1.3 session tickets, here plain bytes)
/// is read before the close, so the server sees an orderly end and not a reset.
#[tokio::test]
async fn closing_reads_what_the_server_still_sent() {
    let listener = TcpListener::bind((LOOPBACK, 0)).await.expect("bind");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await?;
        socket.write_all(&[7; 4096]).await?;
        let mut received = Vec::new();
        socket.read_to_end(&mut received).await?;
        Ok::<_, io::Error>(received)
    });
    let mut client = TcpStream::connect(address).await.expect("connect");
    client.write_all(b"the whole upload").await.expect("upload");
    // The server's bytes arrive before the client closes, unread.
    tokio::time::sleep(Duration::from_millis(50)).await;
    close_data(&mut client).await.expect("closed");
    drop(client);
    let received = server.await.expect("join").expect("an orderly end");
    assert_eq!(received, b"the whole upload");
}
