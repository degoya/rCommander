//! A running forward's listener and the connections it carries (RC-110-03).
//!
//! The listener accepts until its forward is stopped. Each accepted connection is a task of its
//! own: it takes a `direct-tcpip` channel on the connection's session ([`Engine::forward_channel`],
//! which connects when the session is gone) and copies both ways until either side closes or the
//! forward stops. The task holds the engine only while it opens the channel, and the channel's
//! lease for as long as it copies, so the session counts as in use. The count of connections at
//! once is bounded by [`MAX_FORWARD_CONNECTIONS`]; one over it is closed at once and logged.

use std::{
    io,
    net::{IpAddr, SocketAddr},
    sync::{
        Arc, Weak,
        atomic::{AtomicUsize, Ordering},
    },
};

use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::CancellationToken;

use super::{ForwardError, MAX_FORWARD_CONNECTIONS};
use crate::{Engine, Forward, Inner};

/// The listener of `forward`, bound on its address and port.
///
/// # Errors
///
/// [`ForwardError::PortInUse`], or [`ForwardError::BindFailed`] with the system's reason.
pub(crate) async fn bind(forward: &Forward) -> Result<TcpListener, ForwardError> {
    let failed = |reason: String| ForwardError::BindFailed {
        address: forward.bind_address.clone(),
        port: forward.bind_port,
        reason,
    };
    let address: IpAddr = forward
        .bind_address
        .parse()
        .map_err(|_| failed("not an IP address".to_owned()))?;
    TcpListener::bind(SocketAddr::new(address, forward.bind_port))
        .await
        .map_err(|error| match error.kind() {
            io::ErrorKind::AddrInUse => ForwardError::PortInUse {
                address: forward.bind_address.clone(),
                port: forward.bind_port,
            },
            kind => failed(kind.to_string()),
        })
}

/// Accepts on `listener` until `stop` fires, each connection carried by a task of its own.
pub(crate) async fn serve(
    engine: Weak<Inner>,
    forward: Forward,
    listener: TcpListener,
    stop: CancellationToken,
    active: Arc<AtomicUsize>,
) {
    loop {
        let accepted = tokio::select! {
            biased;
            () = stop.cancelled() => break,
            accepted = listener.accept() => accepted,
        };
        let (stream, peer) = match accepted {
            Ok(accepted) => accepted,
            Err(error) => {
                tracing::warn!(forward = forward.id, kind = %error.kind(), "a port forward could not accept a connection");
                continue;
            }
        };
        if active.load(Ordering::SeqCst) >= MAX_FORWARD_CONNECTIONS {
            tracing::warn!(
                forward = forward.id,
                limit = MAX_FORWARD_CONNECTIONS,
                "a port forward refused a connection: too many at once"
            );
            drop(stream);
            continue;
        }
        let counted = Counted::new(Arc::clone(&active));
        tokio::spawn(carry(
            Weak::clone(&engine),
            forward.clone(),
            stream,
            peer,
            stop.child_token(),
            counted,
        ));
    }
    tracing::debug!(forward = forward.id, "port forward stopped");
}

/// Carries one accepted connection over a `direct-tcpip` channel until either side closes or
/// `stop` fires.
async fn carry(
    engine: Weak<Inner>,
    forward: Forward,
    mut stream: TcpStream,
    peer: SocketAddr,
    stop: CancellationToken,
    _counted: Counted,
) {
    let Some(inner) = engine.upgrade() else {
        return;
    };
    let engine = Engine { inner };
    let opened = tokio::select! {
        biased;
        () = stop.cancelled() => return,
        opened = engine.forward_channel(&forward, peer) => opened,
    };
    drop(engine);
    let session = match opened {
        Ok(session) => session,
        Err(error) => {
            tracing::warn!(
                forward = forward.id,
                code = error.code(),
                "a port forward could not reach its target"
            );
            return;
        }
    };
    let mut channel = session.channel.into_stream();
    tokio::select! {
        biased;
        () = stop.cancelled() => {}
        // Either side closing ends it; what the other side still sends is dropped with both.
        _ = copy_bidirectional(&mut stream, &mut channel) => {}
    }
    drop(session.lease);
}

/// One connection counted as running through a forward until it drops.
struct Counted(Arc<AtomicUsize>);

impl Counted {
    fn new(active: Arc<AtomicUsize>) -> Self {
        active.fetch_add(1, Ordering::SeqCst);
        Self(active)
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
