//! Active-mode data connections (`PORT`/`EPRT`), opened here rather than by suppaftp, whose
//! listener binds every interface of the machine; passive ones are `passive.rs`.
//!
//! The listener binds only the local address of the control connection, on a free port, and
//! lives for one transfer. It takes exactly one connection: from the server's control address,
//! or the transfer fails - a connection from anywhere else is closed unread, and nobody gets a
//! second try. Over FTPS the data connection is TLS with the control connection's verifier.
//!
//! [`close_data`] is how every data connection ends, active or passive: `close_notify` and
//! FIN first, then what the server still sends is read until it closes. A TLS 1.3 server sends
//! session tickets after the handshake that an upload never reads; closing a socket with unread
//! bytes makes the kernel answer with a reset, and the server would lose the tail of the upload.

use std::{
    io,
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

/// How long [`close_data`] waits for the server to close after `close_notify`.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
/// How much [`close_data`] reads at most before it closes anyway.
const DRAIN_MAX: usize = 1024 * 1024;

/// The command that names `address` to the server: `PORT h1,h2,h3,h4,p1,p2` for IPv4, `EPRT
/// |2|address|port|` for IPv6 (RFC 959, RFC 2428).
pub(crate) fn port_command(address: SocketAddr) -> String {
    let port = address.port();
    match address.ip() {
        IpAddr::V4(ip) => {
            let [a, b, c, d] = ip.octets();
            format!("PORT {a},{b},{c},{d},{},{}", port >> 8, port & 0xff)
        }
        IpAddr::V6(ip) => format!("EPRT |2|{ip}|{port}|"),
    }
}

/// A listener on `local`, the control connection's local address, on a free port.
pub(crate) async fn listen(local: IpAddr) -> io::Result<TcpListener> {
    TcpListener::bind(SocketAddr::new(local, 0)).await
}

/// Whether `peer` is the server of the control connection; an IPv4-mapped IPv6 address counts
/// as its IPv4 address.
pub(crate) fn is_server(peer: IpAddr, server: IpAddr) -> bool {
    peer.to_canonical() == server.to_canonical()
}

/// The one data connection, from `server` within `timeout`. Anything else fails the transfer.
pub(crate) async fn accept(
    listener: &TcpListener,
    server: IpAddr,
    timeout: Duration,
) -> io::Result<TcpStream> {
    let (socket, peer) = tokio::time::timeout(timeout, listener.accept())
        .await
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "the FTP server did not open the data connection",
            )
        })??;
    if !is_server(peer.ip(), server) {
        drop(socket);
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("a data connection from {peer} is not from the FTP server; refused"),
        ));
    }
    Ok(socket)
}

/// Closes a data connection: `close_notify` (TLS) and FIN, then reads until the server closes,
/// at most [`DRAIN_MAX`] bytes and [`DRAIN_TIMEOUT`]. A read error only ends the draining; a
/// failed shutdown is returned, since an upload is not complete without it.
pub(crate) async fn close_data<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
) -> io::Result<()> {
    match stream.shutdown().await {
        Ok(()) => {}
        // The server closed first: there is nothing left to read.
        Err(error) if error.kind() == io::ErrorKind::NotConnected => return Ok(()),
        Err(error) => return Err(error),
    }
    let drain = async {
        let mut buffer = vec![0; 16 * 1024];
        let mut read = 0;
        while read < DRAIN_MAX {
            match stream.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(count) => read += count,
            }
        }
    };
    // A server that keeps the connection open is not waited for any longer.
    let _ = tokio::time::timeout(DRAIN_TIMEOUT, drain).await;
    Ok(())
}

#[cfg(test)]
#[path = "active_tests.rs"]
mod tests;
