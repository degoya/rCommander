use super::*;

#[test]
fn the_defaults_are_agent_key_password_30_s_and_10_min() {
    let prefs = SessionPrefs::default();
    assert_eq!(prefs.auth_order, AuthKind::DEFAULT_ORDER);
    assert_eq!(prefs.keepalive_secs, 30);
    assert_eq!(prefs.idle_close_secs, 600);
    let read: SessionPrefs = serde_json::from_str("{}").expect("deserializes");
    assert_eq!(read, prefs);
}

#[test]
fn valid_settings_are_stored_as_text() {
    let prefs = SessionPrefs {
        auth_order: vec![AuthKind::Password, AuthKind::Key],
        agent_fingerprint: Some(" SHA256:abc+/def ".to_owned()),
        keepalive_secs: 1,
        idle_close_secs: 0,
    };
    let stored = prefs.checked().expect("valid");
    assert_eq!(stored.auth_order, "password,key");
    assert_eq!(stored.agent_fingerprint.as_deref(), Some("SHA256:abc+/def"));
    let back = SessionPrefs::from_stored(stored);
    assert_eq!(back.auth_order, prefs.auth_order);
    assert_eq!(back.agent_fingerprint.as_deref(), Some("SHA256:abc+/def"));
}

#[test]
fn an_empty_fingerprint_means_every_key() {
    let prefs = SessionPrefs {
        agent_fingerprint: Some("  ".to_owned()),
        ..SessionPrefs::default()
    };
    assert_eq!(prefs.checked().expect("valid").agent_fingerprint, None);
}

#[test]
fn invalid_settings_are_refused() {
    let cases = [
        SessionPrefs {
            auth_order: Vec::new(),
            ..SessionPrefs::default()
        },
        SessionPrefs {
            auth_order: vec![AuthKind::Key, AuthKind::Key],
            ..SessionPrefs::default()
        },
        SessionPrefs {
            keepalive_secs: 0,
            ..SessionPrefs::default()
        },
        SessionPrefs {
            keepalive_secs: MAX_KEEPALIVE_SECS + 1,
            ..SessionPrefs::default()
        },
        SessionPrefs {
            idle_close_secs: MAX_IDLE_CLOSE_SECS + 1,
            ..SessionPrefs::default()
        },
        SessionPrefs {
            agent_fingerprint: Some("MD5:aa:bb".to_owned()),
            ..SessionPrefs::default()
        },
        SessionPrefs {
            agent_fingerprint: Some("SHA256:not base64!".to_owned()),
            ..SessionPrefs::default()
        },
    ];
    for prefs in cases {
        let error = prefs.checked().expect_err("refused");
        assert_eq!(error.code(), "session_prefs_invalid", "{prefs:?}");
    }
}

#[test]
fn a_stored_order_that_does_not_parse_falls_back_to_the_default() {
    let prefs = SessionPrefs::from_stored(StoredSessionPrefs {
        auth_order: "agent,gssapi".to_owned(),
        agent_fingerprint: None,
        keepalive_secs: 0,
        idle_close_secs: 999_999,
    });
    assert_eq!(prefs.auth_order, AuthKind::DEFAULT_ORDER);
    assert_eq!(prefs.keepalive_secs, 1);
    assert_eq!(prefs.idle_close_secs, MAX_IDLE_CLOSE_SECS);
}

#[test]
fn a_slot_with_a_setup_under_way_is_connecting_until_its_guard_drops() {
    let pool = SessionPool::default();
    let id = ConnectionId(7);
    assert_eq!(pool.status(id), None);
    let slot = pool.slot(id);
    let (cancel, guard) = slot.begin_setup();
    let status = pool.status(id).expect("listed");
    assert_eq!(status.state, SshState::Connecting);
    assert_eq!(pool.statuses(), vec![status]);
    let (connecting, live) = slot.close();
    assert!(connecting);
    assert!(live.is_none());
    assert!(cancel.is_cancelled(), "close cancels the setup");
    drop(guard);
    assert_eq!(pool.status(id), None);
    let (fresh, _guard) = slot.begin_setup();
    assert!(!fresh.is_cancelled(), "a later setup gets a fresh token");
}

/// RC-080-47: a slot nobody holds, without a session or a setup, leaves the pool when another
/// is taken; one held or connecting stays.
#[test]
fn an_unused_slot_leaves_the_pool() {
    let pool = SessionPool::default();
    let count = |pool: &SessionPool| lock(&pool.slots).len();
    drop(pool.slot(ConnectionId(1)));
    let held = pool.slot(ConnectionId(2));
    assert_eq!(count(&pool), 1, "slot 1 was unused");
    let connecting = pool.slot(ConnectionId(3));
    let (_cancel, guard) = connecting.begin_setup();
    drop(connecting);
    pool.slot(ConnectionId(4));
    assert_eq!(
        count(&pool),
        3,
        "2 is held, 3 is connecting, 4 was just taken"
    );
    drop(guard);
    drop(held);
    pool.slot(ConnectionId(5));
    assert_eq!(count(&pool), 1, "only 5 is left");
}
