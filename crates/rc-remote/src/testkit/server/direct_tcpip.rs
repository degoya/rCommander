//! The `direct-tcpip` channels a client asks the test server for (RC-110-03): the jump to the next
//! server of a chain, a local port forward's connection. Only with
//! [`super::ServerOptions::direct_tcpip`], and only to a port of 127.0.0.1 - the tests never
//! reach past this machine. Every channel opened is counted ([`super::TestServer::direct_opens`]).
//! A silent echo listener ([`echo_server`]) is the target a forward test carries bytes to.

use std::sync::atomic::{AtomicUsize, Ordering};

use russh::{
    Channel, ChannelOpenFailure,
    server::{ChannelOpenHandle, Msg},
};
use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

use super::ServerOptions;

/// Accepts the channel to `host` on `port` and copies it to a TCP connection there, or refuses
/// it: switched off, a host other than loopback, nothing listening.
pub(super) async fn open(
    options: &ServerOptions,
    opens: &AtomicUsize,
    channel: Channel<Msg>,
    host: &str,
    port: u32,
    reply: ChannelOpenHandle,
) {
    let loopback = matches!(host, "127.0.0.1" | "localhost");
    let Some(port) = u16::try_from(port)
        .ok()
        .filter(|_| options.direct_tcpip && loopback)
    else {
        reply
            .reject(ChannelOpenFailure::AdministrativelyProhibited)
            .await;
        return;
    };
    let Ok(tcp) = TcpStream::connect(("127.0.0.1", port)).await else {
        reply.reject(ChannelOpenFailure::ConnectFailed).await;
        return;
    };
    opens.fetch_add(1, Ordering::SeqCst);
    reply.accept().await;
    tokio::spawn(async move {
        let mut tcp = tcp;
        let mut stream = channel.into_stream();
        // Either side closing ends the copy; what is left is dropped with both.
        let _ = copy_bidirectional(&mut stream, &mut tcp).await;
    });
}

/// A listener on a free port of 127.0.0.1 that echoes every byte back on each connection it
/// accepts, and counts them.
///
/// # Panics
///
/// No port can be bound.
pub async fn echo_server() -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
    let port = listener.local_addr().expect("address").port();
    let task = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let (mut read, mut write) = stream.into_split();
                let _ = tokio::io::copy(&mut read, &mut write).await;
            });
        }
    });
    (port, task)
}
