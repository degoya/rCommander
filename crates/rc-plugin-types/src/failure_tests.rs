use super::{Failure, FailureKind};

#[test]
fn a_failure_shows_its_code_or_its_class() {
    assert_eq!(Failure::new(FailureKind::NotFound).to_string(), "not_found");
    let coded = Failure::coded(FailureKind::BadData, "csv.bad_quote").with_param("line", 7);
    assert_eq!(coded.to_string(), "csv.bad_quote");
    assert_eq!(coded.params.get("line").map(String::as_str), Some("7"));
}

#[test]
fn a_failure_round_trips_through_json() {
    let failure = Failure::coded(FailureKind::WrongPassword, "plugin_suspended").with_param("n", 3);
    let json = serde_json::to_string(&failure).expect("json");
    assert!(json.contains("\"kind\":\"wrong_password\""), "{json}");
    let back: Failure = serde_json::from_str(&json).expect("parse");
    assert_eq!(back, failure);
    let bare: Failure = serde_json::from_str(r#"{"kind":"io"}"#).expect("parse");
    assert_eq!(bare, Failure::new(FailureKind::Io));
}
