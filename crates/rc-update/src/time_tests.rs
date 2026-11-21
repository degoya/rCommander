use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use super::{rfc3339, rfc3339_offset, utc_text};

fn at(seconds: i64, nanos: i32) -> Timestamp {
    Timestamp::new(seconds, nanos).expect("a valid instant")
}

/// The texts chrono wrote for these instants (`to_rfc3339_opts(AutoSi, true)`, `to_rfc3339()`,
/// `Display`), pinned before the move to jiff (RC-110-36).
#[test]
fn the_texts_are_the_ones_chrono_wrote() {
    let cases = [
        (at(1_763_546_400, 0), "2025-11-19T10:00:00Z"),
        (at(1_763_546_400, 250_000_000), "2025-11-19T10:00:00.250Z"),
        (
            at(1_763_546_400, 123_456_000),
            "2025-11-19T10:00:00.123456Z",
        ),
        (
            at(1_763_546_400, 123_456_700),
            "2025-11-19T10:00:00.123456700Z",
        ),
        (at(1_700_000_000, 1), "2023-11-14T22:13:20.000000001Z"),
    ];
    for (instant, text) in cases {
        assert_eq!(rfc3339(instant), text);
    }
    assert_eq!(
        rfc3339_offset(at(1_763_546_400, 0)),
        "2025-11-19T10:00:00+00:00"
    );
    assert_eq!(
        rfc3339_offset(at(1_763_546_400, 250_000_000)),
        "2025-11-19T10:00:00.250+00:00"
    );
    assert_eq!(utc_text(at(1_763_546_400, 0)), "2025-11-19 10:00:00 UTC");
    assert_eq!(
        utc_text(at(1_763_546_400, 5_000_000)),
        "2025-11-19 10:00:00.005 UTC"
    );
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Stored {
    #[serde(with = "super::serde_rfc3339")]
    at: Timestamp,
    #[serde(default, with = "super::serde_rfc3339::option")]
    seen: Option<Timestamp>,
}

#[test]
fn serde_writes_and_reads_the_same_text() {
    let stored = Stored {
        at: at(1_763_546_400, 250_000_000),
        seen: None,
    };
    let text = serde_json::to_string(&stored).expect("writes");
    assert_eq!(text, r#"{"at":"2025-11-19T10:00:00.250Z","seen":null}"#);
    assert_eq!(
        serde_json::from_str::<Stored>(&text).expect("reads"),
        stored
    );
    // Any offset reads, and a missing optional instant is none.
    let other: Stored =
        serde_json::from_str(r#"{"at":"2025-11-19T12:00:00+02:00"}"#).expect("reads");
    assert_eq!(other.at, at(1_763_546_400, 0));
    assert_eq!(other.seen, None);
    let seen: Stored = serde_json::from_str(
        r#"{"at":"2025-11-19T10:00:00Z","seen":"2023-11-14T22:13:20.000000001Z"}"#,
    )
    .expect("reads");
    assert_eq!(seen.seen, Some(at(1_700_000_000, 1)));
    assert!(serde_json::from_str::<Stored>(r#"{"at":"yesterday"}"#).is_err());
}
