//! Local port forwards (RC-110-03) against an in-process SSH server that carries `direct-tcpip`
//! channels to an echo listener on loopback.

use std::time::{Duration, Instant};

use rc_remote::testkit::{TestServer, echo_server};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpStream,
};

use super::super::jump_test_support::{Bench, free_port};
use crate::{
    Command, ConnectionId, Forward, ForwardDraft, ForwardsOutcome, MAX_FORWARD_CONNECTIONS,
    Outcome, SaveForwardRequest,
    settings::{FORWARD_RECONNECT_KEY, FORWARDS_LAN_KEY},
};

/// A bench with a connection to a jump-capable server and an echo listener behind it.
struct Rig {
    bench: Bench,
    server: TestServer,
    id: ConnectionId,
    echo: u16,
    _echo: tokio::task::JoinHandle<()>,
}

async fn rig() -> Rig {
    let bench = Bench::new().await;
    let server = Bench::server(true).await;
    let id = bench.connection("db-box", server.port, None, true).await;
    let (echo, task) = echo_server().await;
    Rig {
        bench,
        server,
        id,
        echo,
        _echo: task,
    }
}

impl Rig {
    async fn save(&self, bind: &str, port: u16, auto_start: bool) -> Forward {
        let request = SaveForwardRequest {
            connection_id: self.id,
            id: None,
            forward: ForwardDraft {
                bind_address: bind.to_owned(),
                bind_port: i64::from(port),
                target_host: "127.0.0.1".to_owned(),
                target_port: i64::from(self.echo),
                auto_start,
                name: None,
            },
        };
        match self
            .bench
            .engine
            .dispatch(Command::SaveForward(request))
            .await
        {
            Ok(Outcome::Forwards(outcome)) => match *outcome {
                ForwardsOutcome::Forward { forward } => forward.forward,
                other => panic!("a forward: {other:?}"),
            },
            other => panic!("saved: {other:?}"),
        }
    }

    async fn start(&self, id: i64) -> Result<Outcome, crate::EngineError> {
        self.bench
            .engine
            .dispatch(Command::StartForward { id })
            .await
    }
}

/// A small running checksum of `bytes` on top of `sum`.
fn checksum(sum: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(sum, |sum, byte| {
        sum.rotate_left(5) ^ u64::from(*byte).wrapping_mul(0x9E37_79B9_7F4A_7C15)
    })
}

/// Sends `total` bytes through `port` and reads them back; the checksums of both ways.
async fn carry(port: u16, total: usize) -> (u64, u64) {
    let stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect");
    let (mut read, mut write) = stream.into_split();
    let sender = tokio::spawn(async move {
        let mut sent = 0u64;
        let mut left = total;
        let mut block = vec![0u8; 64 * 1024];
        let mut counter = 0u8;
        while left > 0 {
            let len = left.min(block.len());
            for byte in &mut block[..len] {
                counter = counter.wrapping_add(31);
                *byte = counter;
            }
            write.write_all(&block[..len]).await.expect("write");
            sent = checksum(sent, &block[..len]);
            left -= len;
        }
        write.shutdown().await.expect("shutdown");
        sent
    });
    let mut received = 0u64;
    let mut got = 0usize;
    let mut buffer = vec![0u8; 64 * 1024];
    while got < total {
        let read_now = read.read(&mut buffer).await.expect("read");
        assert!(read_now > 0, "ended early after {got} bytes");
        received = checksum(received, &buffer[..read_now]);
        got += read_now;
    }
    (sender.await.expect("sender"), received)
}

#[tokio::test]
async fn a_forward_carries_bytes_to_its_target_unchanged() {
    let rig = rig().await;
    let port = free_port();
    let forward = rig.save("", port, false).await;
    assert_eq!(forward.bind_address, "127.0.0.1");
    rig.start(forward.id).await.expect("started");
    let (sent, received) = carry(port, 4 * 1024 * 1024).await;
    assert_eq!(sent, received);
    assert_eq!(rig.server.direct_opens(), 1);
    assert_eq!(
        rig.bench.engine.running_forwards().len(),
        1,
        "the status bar counts it"
    );
}

/// The throughput of a forward against the same `direct-tcpip` channel without a listener, in a
/// release build: `cargo nextest run -p rc-engine --release forward_throughput --run-ignored
/// only`. The verification of RC-110-03 records the ratio (at least 80 %).
#[tokio::test]
#[ignore = "a measurement for the release build"]
async fn forward_throughput() {
    const TOTAL: usize = 100 * 1024 * 1024;
    let rig = rig().await;
    let port = free_port();
    let forward = rig.save("127.0.0.1", port, false).await;
    rig.start(forward.id).await.expect("started");
    let started = Instant::now();
    let (sent, received) = carry(port, TOTAL).await;
    let through_forward = started.elapsed();
    assert_eq!(sent, received, "100 MB unchanged");
    let peer: std::net::SocketAddr = "127.0.0.1:40000".parse().expect("address");
    let session = rig
        .bench
        .engine
        .forward_channel(&forward, peer)
        .await
        .expect("a channel");
    let mut channel = session.channel.into_stream();
    let started = Instant::now();
    let block = vec![7u8; 64 * 1024];
    let mut buffer = vec![0u8; 64 * 1024];
    let mut got = 0usize;
    let (mut read, mut write) = tokio::io::split(&mut channel);
    let writer = async {
        for _ in 0..TOTAL / block.len() {
            write.write_all(&block).await.expect("write");
        }
    };
    let reader = async {
        while got < TOTAL {
            got += read.read(&mut buffer).await.expect("read");
        }
    };
    tokio::join!(writer, reader);
    let direct = started.elapsed();
    let ratio = direct.as_secs_f64() / through_forward.as_secs_f64();
    println!("forward {through_forward:?}, channel {direct:?}, ratio {ratio:.2}");
    assert!(
        ratio >= 0.8,
        "the forward reached {ratio:.2} of the channel"
    );
}

#[tokio::test]
async fn sixty_four_connections_run_and_the_next_is_refused() {
    let rig = rig().await;
    let port = free_port();
    let forward = rig.save("127.0.0.1", port, false).await;
    rig.start(forward.id).await.expect("started");
    let mut held = Vec::new();
    for _ in 0..MAX_FORWARD_CONNECTIONS {
        let mut stream = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect");
        stream.write_all(b"x").await.expect("write");
        let mut one = [0u8; 1];
        stream.read_exact(&mut one).await.expect("echoed");
        held.push(stream);
    }
    let mut extra = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("accepted");
    let mut buffer = [0u8; 1];
    let closed = tokio::time::timeout(Duration::from_secs(5), extra.read(&mut buffer))
        .await
        .expect("closed at once");
    assert!(matches!(closed, Ok(0) | Err(_)), "{closed:?}");
    let started = Instant::now();
    rig.bench
        .engine
        .dispatch_mcp(Command::StopForward { id: forward.id })
        .await
        .expect("stopped");
    for mut stream in held {
        let ended = tokio::time::timeout(Duration::from_secs(1), stream.read(&mut buffer))
            .await
            .expect("closed within a second");
        assert!(matches!(ended, Ok(0) | Err(_)), "{ended:?}");
    }
    assert!(started.elapsed() < Duration::from_secs(2));
    // The forward is gone; whether the port answers is another test's business by now.
    assert!(rig.bench.engine.running_forwards().is_empty());
}

#[tokio::test]
async fn a_lan_address_is_refused_without_the_setting_and_a_taken_port_is_named() {
    let rig = rig().await;
    let port = free_port();
    let lan = rig.save("0.0.0.0", port, false).await;
    let error = rig.start(lan.id).await.expect_err("refused");
    assert_eq!(error.code(), "forward_bind_refused");
    // No socket was opened. The port itself says nothing: another test may hold it by now.
    assert!(rig.bench.engine.running_forwards().is_empty());
    rig.bench.set(FORWARDS_LAN_KEY, true).await;
    let taken = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let busy = rig
        .save(
            "127.0.0.1",
            taken.local_addr().expect("address").port(),
            false,
        )
        .await;
    let error = rig.start(busy.id).await.expect_err("in use");
    assert_eq!(error.code(), "forward_port_in_use");
}

#[tokio::test]
async fn auto_start_follows_the_session_and_a_disconnect_stops_it() {
    let rig = rig().await;
    let port = free_port();
    rig.save("127.0.0.1", port, true).await;
    rig.bench
        .engine
        .dispatch(Command::ConnectSession { id: rig.id })
        .await
        .expect("connected");
    assert_eq!(rig.bench.engine.running_forwards().len(), 1);
    let (sent, received) = carry(port, 1024).await;
    assert_eq!(sent, received);
    rig.bench
        .engine
        .dispatch(Command::DisconnectSession { id: rig.id })
        .await
        .expect("disconnected");
    assert!(rig.bench.engine.running_forwards().is_empty());
}

#[tokio::test]
async fn an_agents_connect_starts_no_forward() {
    let rig = rig().await;
    rig.save("127.0.0.1", free_port(), true).await;
    rig.bench
        .engine
        .dispatch_mcp(Command::ConnectSession { id: rig.id })
        .await
        .expect("connected");
    assert!(rig.bench.engine.running_forwards().is_empty());
}

#[tokio::test]
async fn a_loss_stops_the_forwards_only_without_reconnect() {
    for reconnect in [true, false] {
        let rig = rig().await;
        rig.bench.set(FORWARD_RECONNECT_KEY, reconnect).await;
        rig.bench
            .engine
            .dispatch(Command::SetSessionPrefs {
                id: rig.id,
                prefs: crate::SessionPrefs {
                    keepalive_secs: 1,
                    ..crate::SessionPrefs::default()
                },
            })
            .await
            .expect("keepalive");
        let port = free_port();
        rig.save("127.0.0.1", port, true).await;
        rig.bench
            .engine
            .dispatch(Command::ConnectSession { id: rig.id })
            .await
            .expect("connected");
        rig.server.freeze();
        tokio::time::timeout(Duration::from_secs(10), async {
            while rig.bench.engine.ssh_session(rig.id).is_some() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("lost");
        tokio::time::sleep(Duration::from_millis(200)).await;
        let running = rig.bench.engine.running_forwards().len();
        assert_eq!(running, usize::from(reconnect), "reconnect {reconnect}");
        if reconnect {
            // The next connection connects the session again.
            let (sent, received) = carry(port, 1024).await;
            assert_eq!(sent, received);
            assert_eq!(rig.server.accepts(), 2);
        }
    }
}

#[tokio::test]
async fn an_agent_cannot_start_or_save_a_forward() {
    let rig = rig().await;
    let port = free_port();
    let forward = rig.save("127.0.0.1", port, false).await;
    let error = rig
        .bench
        .engine
        .dispatch_mcp(Command::StartForward { id: forward.id })
        .await
        .expect_err("gui only");
    assert_eq!(error.code(), "gui_only");
    // No socket was opened; the port itself may be another test's by now.
    assert!(rig.bench.engine.running_forwards().is_empty());
    let save = Command::SaveForward(SaveForwardRequest {
        connection_id: rig.id,
        id: Some(forward.id),
        forward: ForwardDraft {
            bind_address: "0.0.0.0".to_owned(),
            bind_port: 8080,
            target_host: "db".to_owned(),
            target_port: 5432,
            auto_start: true,
            name: None,
        },
    });
    let error = rig
        .bench
        .engine
        .dispatch_mcp(save)
        .await
        .expect_err("gui only");
    assert_eq!(error.code(), "gui_only");
    let listed = rig
        .bench
        .engine
        .dispatch_mcp(Command::ListForwards {
            connection_id: Some(rig.id),
        })
        .await
        .expect("listed");
    let Outcome::Forwards(outcome) = listed else {
        panic!("forwards");
    };
    let ForwardsOutcome::Listed { forwards } = *outcome else {
        panic!("a list");
    };
    assert_eq!(forwards.len(), 1);
    assert!(!forwards[0].running);
    rig.bench
        .engine
        .dispatch_mcp(Command::DeleteForward { id: forward.id })
        .await
        .expect("an agent deletes");
}
