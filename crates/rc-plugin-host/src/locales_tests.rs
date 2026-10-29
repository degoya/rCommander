use super::{MAX_LOCALE_BYTES, locale_member_language, parse_locale, validate_locales};

fn bytes(value: &str) -> Vec<u8> {
    value.as_bytes().to_vec()
}

#[test]
fn member_names_map_to_language_tags() {
    assert_eq!(locale_member_language("locales/de.json"), Some("de"));
    assert_eq!(locale_member_language("locales/en.json"), Some("en"));
    assert_eq!(locale_member_language("locales/EN.json"), None);
    assert_eq!(locale_member_language("locales/deu.json"), None);
    assert_eq!(locale_member_language("locales/../evil.json"), None);
    assert_eq!(locale_member_language("component.wasm"), None);
}

#[test]
fn codes_must_use_the_plugins_namespace() {
    let good = bytes(r#"{"codes":{"csv.bad_quote":"A quote is not closed"}}"#);
    assert!(parse_locale("csv", "en", &good).is_ok());
    let host = bytes(r#"{"codes":{"plugin_crashed":"hijacked"}}"#);
    assert!(parse_locale("csv", "en", &host).is_err());
    let foreign = bytes(r#"{"codes":{"json.bad":"nope"}}"#);
    assert!(parse_locale("csv", "en", &foreign).is_err());
    let empty = bytes(r#"{"codes":{"csv.bad_quote":"  "}}"#);
    assert!(parse_locale("csv", "en", &empty).is_err());
}

#[test]
fn unknown_fields_are_rejected() {
    assert!(parse_locale("csv", "en", &bytes(r#"{"account":{}}"#)).is_err());
}

#[test]
fn english_is_required_when_locales_are_present() {
    let fr = vec![("fr".to_owned(), bytes(r#"{"name":"Tableau CSV"}"#))];
    assert!(validate_locales("csv", &fr).is_err());
    let both = vec![
        ("fr".to_owned(), bytes(r#"{"name":"Tableau CSV"}"#)),
        ("en".to_owned(), bytes(r#"{"name":"CSV table"}"#)),
    ];
    assert!(validate_locales("csv", &both).is_ok());
    assert!(validate_locales("csv", &[]).is_ok());
}

#[test]
fn a_locale_over_its_size_or_count_is_refused() {
    let big = vec![(
        "en".to_owned(),
        vec![b' '; usize::try_from(MAX_LOCALE_BYTES).expect("fits") + 1],
    )];
    assert!(validate_locales("csv", &big).is_err());
    let many: Vec<_> = (b'a'..=b'q')
        .map(|letter| (format!("{}n", letter as char), bytes("{}")))
        .collect();
    assert!(validate_locales("csv", &many).is_err());
}
