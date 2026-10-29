use url::Url;

use super::{redact_text, redact_url};

const SECRET: &str = "s3cr3t-value-0123";

#[test]
fn an_address_loses_user_info_and_secret_query_values() {
    let url = Url::parse(&format!(
        "ftp://alice:{SECRET}@files.example/pub?token={SECRET}&page=2"
    ))
    .expect("url");
    let redacted = redact_url(&url);
    assert!(!redacted.contains(SECRET), "{redacted}");
    assert!(redacted.contains("token=%5Bredacted%5D") || redacted.contains("token=[redacted]"));
    assert!(redacted.contains("page=2"), "{redacted}");
}

#[test]
fn prose_loses_headers_bearer_values_and_assignments() {
    let text = format!(
        "Authorization: Bearer {SECRET}\n\
         sent bearer {SECRET} to sftp://bob:{SECRET}@host.example/home.\n\
         login with password={SECRET} and user=bob; API_KEY={SECRET}"
    );
    let redacted = redact_text(&text);
    assert!(!redacted.contains(SECRET), "{redacted}");
    assert!(redacted.contains("password=[redacted]"), "{redacted}");
    assert!(redacted.contains("user=bob"), "{redacted}");
    assert!(redacted.contains("API_KEY=[redacted]"), "{redacted}");
    assert!(redacted.ends_with("[redacted]"), "{redacted}");
}

#[test]
fn nothing_to_hide_is_byte_for_byte_and_twice_is_once() {
    for text in [
        "C:\\Users\\bob\\notes.txt",
        "/home/bob/keyring/monkey=banana.txt",
        "https://Example.COM/a?page=2",
        "member basic-ness",
    ] {
        assert_eq!(redact_text(text), text);
    }
    let once = redact_text(&format!("password={SECRET} https://u:{SECRET}@h.example/"));
    assert_eq!(redact_text(&once), once);
}

/// The credential variables of the common clouds and forges, as an environment or a `.env`
/// file writes them.
#[test]
fn cloud_and_forge_credentials_are_redacted() {
    let text = format!(
        "AWS_SECRET_ACCESS_KEY={SECRET} aws_session_token={SECRET}\n\
         GITHUB_TOKEN={SECRET} refresh_token={SECRET} region=eu-west-1"
    );
    let redacted = redact_text(&text);
    assert!(!redacted.contains(SECRET), "{redacted}");
    assert!(
        redacted.contains("AWS_SECRET_ACCESS_KEY=[redacted]"),
        "{redacted}"
    );
    assert!(redacted.contains("region=eu-west-1"), "{redacted}");
}

/// RC-080-42, the owner's decision of 2026-10-28: a secret's value is found in every form a file,
/// a terminal or a server writes it - one case per form - and a second pass changes nothing.
#[test]
fn every_form_of_a_secret_after_its_name_is_redacted() {
    for (text, expected) in [
        // key = value
        (
            format!("password = {SECRET}"),
            "password = [redacted]".to_owned(),
        ),
        // key: value (YAML)
        (
            format!("password: {SECRET}"),
            "password: [redacted]".to_owned(),
        ),
        (
            format!("  db_password: {SECRET}  # prod"),
            "  db_password: [redacted]  # prod".to_owned(),
        ),
        // A name with a secret's part.
        (
            format!("export DB_PASSWORD={SECRET}"),
            "export DB_PASSWORD=[redacted]".to_owned(),
        ),
        (
            format!("MY_API_TOKEN: {SECRET}"),
            "MY_API_TOKEN: [redacted]".to_owned(),
        ),
        (
            format!("spring.datasource.password={SECRET}"),
            "spring.datasource.password=[redacted]".to_owned(),
        ),
        // "password":"x" (JSON), quoted values.
        (
            format!("{{\"password\":\"{SECRET}\"}}"),
            "{\"password\":\"[redacted]\"}".to_owned(),
        ),
        (
            format!("{{\"password\": \"{SECRET}\", \"user\": \"bob\"}}"),
            "{\"password\": \"[redacted]\", \"user\": \"bob\"}".to_owned(),
        ),
        (
            format!("'api_token': '{SECRET}'"),
            "'api_token': '[redacted]'".to_owned(),
        ),
        (
            format!("password=\"{SECRET} with blanks\""),
            "password=\"[redacted]\"".to_owned(),
        ),
        (
            format!("secret: \"a \\\"quoted\\\" {SECRET}\""),
            "secret: \"[redacted]\"".to_owned(),
        ),
    ] {
        let redacted = redact_text(&text);
        assert_eq!(redacted, expected, "{text}");
        assert_eq!(redact_text(&redacted), redacted, "twice is once: {text}");
    }
}

/// A credential header inside a line, as the error log stores a tracing field: the scheme's rule
/// takes the credential after `Bearer`, the colon form does not stop at the scheme word.
#[test]
fn a_header_inside_a_line_loses_its_credential() {
    let text = format!("proxy said no header=Authorization: Bearer {SECRET}");
    let redacted = redact_text(&text);
    assert_eq!(
        redacted,
        "proxy said no header=Authorization: Bearer [redacted]"
    );
    assert_eq!(redact_text(&redacted), redacted);
    assert_eq!(
        redact_text(&format!("auth_token: Basic {SECRET}")),
        "auth_token: Basic [redacted]"
    );
}

/// The colon forms leave names too common to be a credential's, and structured values; prompts
/// without a value stay. Documentation is masked - the price of the rule, as decided.
#[test]
fn the_colon_forms_leave_the_ambiguous_and_the_structured() {
    for text in [
        "key: F5",
        "session: 3",
        "signature: good",
        "\"has_password\": true",
        "\"secrets\": [{\"id\": 1}]",
        "password: null",
        "max_tokens=1000",
        "Password:",
        "[sudo] password for bob:",
        "Note: see the manual",
        "(?i)token:",
    ] {
        assert_eq!(redact_text(text), text);
    }
    assert_eq!(
        redact_text("password: see vault"),
        "password: [redacted] vault"
    );
}
