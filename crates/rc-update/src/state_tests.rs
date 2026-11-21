use super::*;

#[test]
fn a_state_round_trips_and_an_empty_or_broken_one_reads_as_fresh() {
    let mut state = UpdateState::default();
    state.floors.stable = Some(5);
    state.last_code = Some("update.fetch_failed".to_owned());
    let value = serde_json::to_value(&state).expect("value");
    assert_eq!(UpdateState::from_value(Some(value)), state);
    assert_eq!(UpdateState::from_value(None), UpdateState::default());
    assert_eq!(
        UpdateState::from_value(Some(serde_json::json!({ "floors": "nonsense" }))),
        UpdateState::default()
    );
    // A state from an older build without the newer fields still reads.
    let old = UpdateState::from_value(Some(serde_json::json!({ "floors": { "beta": 3 } })));
    assert_eq!(old.floors.beta, Some(3));
}

#[test]
fn a_failed_version_is_remembered_once_and_the_list_is_bounded() {
    let mut state = UpdateState::default();
    state.record_failure("1.2.0");
    state.record_failure("1.2.0");
    assert_eq!(state.failed, vec!["1.2.0".to_owned()]);
    assert!(state.failed_before("1.2.0"));
    for minor in 0..20 {
        state.record_failure(&format!("2.{minor}.0"));
    }
    assert_eq!(state.failed.len(), 8);
    assert!(!state.failed_before("1.2.0"));
}

/// The stored setting keeps its text: a state chrono wrote reads, and writes back the same
/// (RC-110-36).
#[test]
fn a_stored_state_keeps_its_dates() {
    let stored = serde_json::json!({
        "floors": { "stable": 1_790_000_000_u64, "beta": null },
        "last_check": "2026-09-21T14:13:20.123456700Z",
        "last_code": null,
        "offer": {
            "version": "1.8.0",
            "channel": "stable",
            "released_at": "2026-09-21T14:13:20Z",
            "notes": "",
            "artifact": null,
            "schema_change": true
        },
        "failed": [],
        "announced": null
    });
    let state = UpdateState::from_value(Some(stored.clone()));
    assert_eq!(
        state.last_check,
        Some(Timestamp::new(1_790_000_000, 123_456_700).expect("instant"))
    );
    assert_eq!(serde_json::to_value(&state).expect("value"), stored);
}
