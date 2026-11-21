use super::*;

fn at(minutes: i64) -> Timestamp {
    Timestamp::from_second(1_760_000_000 + minutes * 60).expect("timestamp")
}

fn fresh(sequence: u64) -> Freshness {
    Freshness {
        sequence,
        issued_at: at(0),
        not_after: Some(at(60)),
    }
}

#[test]
fn a_newer_sequence_is_accepted() {
    assert_eq!(check(fresh(8), Some(7), at(1)), Ok(()));
}

/// The replay case: yesterday's genuine manifest, served again.
#[test]
fn the_same_or_an_older_sequence_is_a_replay() {
    for sequence in [6, 7] {
        assert!(matches!(
            check(fresh(sequence), Some(7), at(1)),
            Err(StaleError::Replayed { .. })
        ));
    }
}

#[test]
fn a_first_contact_accepts_any_sequence_but_not_an_expired_document() {
    assert_eq!(check(fresh(1), None, at(1)), Ok(()));
    assert!(matches!(
        check(fresh(1), None, at(61)),
        Err(StaleError::Expired { .. })
    ));
}

#[test]
fn clock_skew_within_the_allowance_is_tolerated_and_next_week_is_not() {
    let skewed = Freshness {
        sequence: 1,
        issued_at: at(30),
        not_after: None,
    };
    assert_eq!(check(skewed, None, at(0)), Ok(()));
    let future = Freshness {
        sequence: 1,
        issued_at: at(60 * 24 * 7),
        not_after: None,
    };
    assert!(matches!(
        check(future, None, at(0)),
        Err(StaleError::FromTheFuture { .. })
    ));
}

/// The texts of the refusals are the ones chrono's `Display` wrote (RC-110-36).
#[test]
fn a_refusal_names_its_instant_as_before() {
    let expired = check(fresh(1), None, at(61)).expect_err("expired");
    assert_eq!(
        expired.to_string(),
        "document expired at 2025-10-09 09:53:20 UTC"
    );
    let future = Freshness {
        sequence: 1,
        issued_at: at(60 * 24),
        not_after: None,
    };
    assert_eq!(
        check(future, None, at(0)).expect_err("ahead").to_string(),
        "document is issued 2025-10-10 08:53:20 UTC, further ahead than clock skew explains"
    );
}
