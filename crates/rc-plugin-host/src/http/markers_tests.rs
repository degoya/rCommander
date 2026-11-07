use secrecy::SecretString;

use super::{MASK, fill, mask, named, named_in};
use crate::failure::codes;

const CANARY: &str = "canary-token-5521";

fn slots() -> Vec<(String, SecretString)> {
    vec![("api_token".to_owned(), SecretString::from(CANARY))]
}

#[test]
fn a_marker_is_filled_with_its_slot() {
    let filled = fill("Token {{secret:api_token}}", &slots()).expect("filled");
    assert_eq!(filled.as_str(), format!("Token {CANARY}"));
    let twice = fill("{{secret:api_token}}:{{secret:api_token}}", &slots()).expect("filled");
    assert_eq!(twice.as_str(), format!("{CANARY}:{CANARY}"));
    let plain = fill("application/json", &slots()).expect("unchanged");
    assert_eq!(plain.as_str(), "application/json");
}

/// A slot without a value and a marker that does not close are refused, never filled empty.
#[test]
fn an_unknown_or_open_marker_is_missing() {
    for value in [
        "Token {{secret:other}}",
        "Token {{secret:api_token",
        "Token {{secret:}}",
    ] {
        let failure = fill(value, &slots()).expect_err(value);
        assert_eq!(
            failure.code(),
            Some(codes::PLUGIN_SECRET_MISSING),
            "{value}"
        );
    }
    let failure = fill("Token {{secret:api_token}}", &[]).expect_err("no slots");
    assert_eq!(failure.code(), Some(codes::PLUGIN_SECRET_MISSING));
}

#[test]
fn a_marker_is_found_also_percent_encoded() {
    assert!(named("https://netbox.example.net/?t={{secret:api_token}}"));
    assert!(named(
        "https://netbox.example.net/%7b%7bsecret:api_token%7D%7D"
    ));
    assert!(!named("https://netbox.example.net/api/dcim/devices/"));
    assert!(named_in(b"{\"token\":\"{{secret:api_token}}\"}"));
    assert!(!named_in(b"{}"));
}

#[test]
fn a_value_is_masked_wherever_it_appears() {
    let echoed = format!("401: token {CANARY} is invalid ({CANARY})");
    let masked = mask(&echoed, &slots());
    assert!(!masked.contains(CANARY), "{masked}");
    assert_eq!(masked, format!("401: token {MASK} is invalid ({MASK})"));
    let short = vec![("pin".to_owned(), SecretString::from("ab"))];
    assert_eq!(mask("abc", &short), "abc");
}
