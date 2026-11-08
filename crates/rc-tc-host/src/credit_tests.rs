use super::*;

const W: u64 = WINDOW as u64;

#[test]
fn the_sender_waits_when_the_window_is_spent() {
    let mut out = Outbound::default();
    assert!(out.spend(W - 10));
    assert!(!out.spend(11));
    assert_eq!(out.available(), 10);
    out.grant(W - 10).expect("returned what was sent");
    assert!(out.spend(W));
}

#[test]
fn credit_never_sent_is_a_violation() {
    let mut out = Outbound::default();
    assert!(out.spend(5));
    assert_eq!(out.grant(6), Err(CreditError::Overgrant(1)));
    assert_eq!(out.available(), W - 5);
}

#[test]
fn the_receiver_refuses_bytes_beyond_the_window() {
    let mut inbound = Inbound::default();
    inbound.receive(W).expect("the whole window");
    assert_eq!(inbound.receive(1), Err(CreditError::Overrun(1)));
    inbound.release(W);
    inbound.receive(W).expect("the window again after release");
}
