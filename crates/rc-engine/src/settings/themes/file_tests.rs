use std::collections::BTreeMap;

use serde_json::json;

use super::{MAX_FILE_BYTES, ThemeFileError, parse_file, write_file};
use crate::settings::themes::{Theme, ThemeAccent};

fn bytes(value: &serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("json")
}

fn valid() -> serde_json::Value {
    json!({
        "format": "rcommander-theme",
        "version": 1,
        "name": "Night",
        "dark": true,
        "accent": "#FF8000",
        "tokens": {"window": "#101010", "terminal.red": "#ff0000"}
    })
}

#[test]
fn a_valid_file_reads_whole() {
    let theme = parse_file(&bytes(&valid())).expect("valid");
    assert_eq!(theme.name, "Night");
    assert!(theme.dark);
    assert_eq!(theme.accent, ThemeAccent::Colour("#ff8000".into()));
    assert_eq!(
        theme.tokens.get("window").map(String::as_str),
        Some("#101010")
    );
    assert_eq!(theme.tokens.len(), 2);
    assert!(theme.dropped.is_empty());
}

#[test]
fn what_is_written_reads_back() {
    let mut tokens = BTreeMap::new();
    tokens.insert("list_fill".to_owned(), "#000000".to_owned());
    let theme = Theme {
        id: "own-1".into(),
        name: "Ink".into(),
        dark: false,
        accent: ThemeAccent::System,
        tokens: tokens.clone(),
        builtin: false,
    };
    let read = parse_file(&write_file(&theme)).expect("reads back");
    assert_eq!((read.name.as_str(), read.dark), ("Ink", false));
    assert_eq!(read.accent, ThemeAccent::System);
    assert_eq!(read.tokens, tokens);
}

#[test]
fn a_file_too_large_is_refused_before_it_is_read() {
    let large = vec![b' '; MAX_FILE_BYTES + 1];
    assert_eq!(parse_file(&large), Err(ThemeFileError::TooLarge));
}

#[test]
fn a_wrong_colour_refuses_the_file() {
    for (token, value) in [
        ("window", json!("red")),
        ("window", json!("#12345")),
        ("window", json!("#1234567")),
        ("window", json!(12)),
    ] {
        let mut file = valid();
        file["tokens"][token] = value.clone();
        assert_eq!(
            parse_file(&bytes(&file)),
            Err(ThemeFileError::Colour),
            "{value}"
        );
    }
    let mut file = valid();
    file["accent"] = json!("orange");
    assert_eq!(parse_file(&bytes(&file)), Err(ThemeFileError::Colour));
}

#[test]
fn an_unknown_key_refuses_the_file_and_an_unknown_token_is_dropped() {
    let mut file = valid();
    file["script"] = json!("rm -rf /");
    assert_eq!(parse_file(&bytes(&file)), Err(ThemeFileError::UnknownKey));

    let mut file = valid();
    file["tokens"]["sparkle"] = json!("#ffffff");
    let theme = parse_file(&bytes(&file)).expect("read without it");
    assert_eq!(theme.dropped, ["sparkle"]);
    assert!(!theme.tokens.contains_key("sparkle"));
}

#[test]
fn format_version_and_name_are_checked() {
    let mut file = valid();
    file["format"] = json!("something-else");
    assert!(matches!(
        parse_file(&bytes(&file)),
        Err(ThemeFileError::Invalid(_))
    ));
    let mut file = valid();
    file["version"] = json!(2);
    assert_eq!(parse_file(&bytes(&file)), Err(ThemeFileError::Version));
    let mut file = valid();
    file["name"] = json!(" \u{7}\n ");
    assert!(matches!(
        parse_file(&bytes(&file)),
        Err(ThemeFileError::Invalid(_))
    ));
    assert!(matches!(
        parse_file(b"not json"),
        Err(ThemeFileError::Invalid(_))
    ));
}

/// The name loses its control characters and is cut to the name rule.
#[test]
fn the_name_is_cleaned() {
    let mut file = valid();
    file["name"] = json!(format!("Night\u{1b}[31m{}", "x".repeat(100)));
    let theme = parse_file(&bytes(&file)).expect("valid");
    assert!(!theme.name.chars().any(char::is_control));
    assert_eq!(theme.name.chars().count(), 64);
    assert!(theme.name.starts_with("Night[31m"));
}

#[test]
fn every_error_has_its_code() {
    let errors = [
        ThemeFileError::TooLarge,
        ThemeFileError::Invalid("x"),
        ThemeFileError::UnknownKey,
        ThemeFileError::Version,
        ThemeFileError::Colour,
    ];
    let codes: Vec<&str> = errors.iter().map(ThemeFileError::code).collect();
    assert_eq!(codes, ThemeFileError::ALL_CODES);
}
