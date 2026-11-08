//! The forward desk and its small rules (RC-110-03); the forwards at work are in
//! `commands/forwards_tests.rs`.

use std::sync::{Arc, atomic::AtomicUsize};

use tokio_util::sync::CancellationToken;

use super::{ForwardDesk, ForwardError, Running, is_loopback};
use crate::{ConnectionId, Forward};

fn forward(id: i64, connection: i64) -> Forward {
    Forward {
        id,
        connection_id: ConnectionId(connection),
        bind_address: "127.0.0.1".to_owned(),
        bind_port: 15_000,
        target_host: "db".to_owned(),
        target_port: 5432,
        auto_start: false,
        name: None,
        sort_order: 0,
    }
}

fn running(forward: Forward) -> (Running, CancellationToken) {
    let stop = CancellationToken::new();
    (
        Running {
            forward,
            stop: stop.clone(),
            active: Arc::new(AtomicUsize::new(0)),
        },
        stop,
    )
}

#[test]
fn only_loopback_counts_as_loopback() {
    for address in ["127.0.0.1", "::1", "127.0.0.2"] {
        assert!(is_loopback(address), "{address}");
    }
    for address in ["0.0.0.0", "::", "192.168.1.10", "localhost", ""] {
        assert!(!is_loopback(address), "{address}");
    }
}

#[test]
fn the_desk_keeps_one_per_forward_and_stops_by_connection() {
    let desk = ForwardDesk::default();
    let (one, one_stop) = running(forward(1, 7));
    let (two, two_stop) = running(forward(2, 8));
    assert!(desk.insert(one));
    assert!(desk.insert(two));
    let (again, _) = running(forward(1, 7));
    assert!(!desk.insert(again), "it runs already");
    assert_eq!(
        desk.view(forward(1, 7)).label,
        "127.0.0.1:15000 \u{2192} db:5432"
    );
    assert!(desk.view(forward(1, 7)).running);
    let stopped = desk.stop_all(Some(ConnectionId(7)));
    assert_eq!(stopped.iter().map(|f| f.id).collect::<Vec<_>>(), [1]);
    assert!(one_stop.is_cancelled());
    assert!(!two_stop.is_cancelled());
    assert_eq!(desk.running().len(), 1);
    assert!(desk.stop(2).is_some());
    assert!(two_stop.is_cancelled());
    assert!(desk.stop(2).is_none());
}

#[test]
fn every_failure_has_its_code() {
    let errors = [
        ForwardError::BindRefused {
            address: "0.0.0.0".to_owned(),
        },
        ForwardError::PortInUse {
            address: "127.0.0.1".to_owned(),
            port: 80,
        },
        ForwardError::BindFailed {
            address: "127.0.0.1".to_owned(),
            port: 80,
            reason: "permission denied".to_owned(),
        },
    ];
    let codes: Vec<&str> = errors.iter().map(ForwardError::code).collect();
    assert_eq!(codes, ForwardError::ALL_CODES);
}
