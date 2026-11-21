use jiff::{Timestamp, civil::date};
use serde::{Deserialize, Serialize};

use super::{instant_text, local_text};

/// The texts chrono's serde wrote for a `NaiveDateTime` and a `DateTime<Utc>`, pinned before
/// the move to jiff (RC-110-36).
#[test]
fn the_texts_are_the_ones_chrono_wrote() {
    let morning = date(2026, 11, 7).at(8, 15, 0, 0);
    assert_eq!(local_text(morning), "2026-11-07T08:15:00");
    assert_eq!(
        local_text(date(2026, 11, 7).at(8, 15, 0, 250_000_000)),
        "2026-11-07T08:15:00.250"
    );
    assert_eq!(
        local_text(date(2026, 11, 7).at(8, 15, 0, 123_456_700)),
        "2026-11-07T08:15:00.123456700"
    );
    let at = Timestamp::from_second(1_700_000_000).expect("instant");
    assert_eq!(instant_text(at), "2023-11-14T22:13:20Z");
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Moments {
    #[serde(default, with = "super::local_option")]
    opened: Option<jiff::civil::DateTime>,
    #[serde(with = "super::instant")]
    started: Timestamp,
}

#[test]
fn serde_writes_and_reads_the_same_text() {
    let moments = Moments {
        opened: Some(date(2026, 11, 7).at(8, 15, 0, 0)),
        started: Timestamp::from_second(1_700_000_000).expect("instant"),
    };
    let text = serde_json::to_string(&moments).expect("writes");
    assert_eq!(
        text,
        r#"{"opened":"2026-11-07T08:15:00","started":"2023-11-14T22:13:20Z"}"#
    );
    assert_eq!(
        serde_json::from_str::<Moments>(&text).expect("reads"),
        moments
    );
    let none: Moments =
        serde_json::from_str(r#"{"started":"2023-11-14T23:13:20+01:00"}"#).expect("reads");
    assert_eq!(none.opened, None);
    assert_eq!(none.started, moments.started);
    assert!(
        serde_json::from_str::<Moments>(r#"{"opened":"soon","started":"2023-11-14T22:13:20Z"}"#)
            .is_err()
    );
}
