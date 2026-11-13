use rmcp::model::{CallToolResponse, CallToolResult, ContentBlock, InputRequiredResult};

use super::{mask_response, mask_value, redact_string};

const SECRET: &str = "0123456789abcdef-planted";

fn text_of(answer: Result<CallToolResponse, rmcp::ErrorData>) -> String {
    match answer {
        Ok(CallToolResponse::Complete(result)) => match &result.content[0] {
            ContentBlock::Text(content) => content.text.clone(),
            _ => panic!("a text block"),
        },
        _ => panic!("a complete answer"),
    }
}

#[test]
fn a_planted_secret_is_masked_at_any_depth_and_a_path_stays() {
    let mut answer = serde_json::json!({
        "entries": [{
            "name": "notes.txt",
            "path": "C:\\Users\\bob\\notes.txt",
            "remote": format!("sftp://bob:{SECRET}@host.example/home"),
            "note": format!("fetched https://cdn.example/f?token={SECRET} ok"),
        }],
        "password": SECRET,
        "header": format!("Authorization: Bearer {SECRET}"),
    });
    let before = answer.clone();
    assert!(mask_value(&mut answer));
    let rendered = answer.to_string();
    assert!(!rendered.contains(SECRET), "{rendered}");
    assert_eq!(answer["entries"][0]["path"], before["entries"][0]["path"]);
    assert_eq!(answer["entries"][0]["name"], "notes.txt");
    assert_eq!(answer["password"], "[redacted]");
}

#[test]
fn an_answer_with_nothing_to_hide_is_not_rewritten() {
    // Compact on purpose: a rewrite would make it pretty, so equality proves there was none.
    let text = r#"{"path":"/home/bob","url":"https://Example.COM"}"#;
    let answer = Ok(CallToolResult::success(vec![ContentBlock::text(text)]).into());
    assert_eq!(text_of(mask_response(answer, &|_: &str| None)), text);
}

#[test]
fn text_structured_content_and_errors_are_all_masked() {
    let address = format!("ftp://alice:{SECRET}@files.example/");
    let mut result = CallToolResult::success(vec![ContentBlock::text(format!(
        "{{\"url\":\"{address}\"}}"
    ))]);
    result.structured_content = Some(serde_json::json!({ "url": address }));
    let Ok(CallToolResponse::Complete(masked)) = mask_response(Ok(result.into()), &|_: &str| None)
    else {
        panic!("a complete answer");
    };
    let rendered = serde_json::to_string(&masked).expect("serializes");
    assert!(!rendered.contains(SECRET), "{rendered}");

    let prose = Ok(CallToolResult::success(vec![ContentBlock::text(format!(
        "could not log in with password={SECRET}"
    ))])
    .into());
    assert!(!text_of(mask_response(prose, &|_: &str| None)).contains(SECRET));

    let error = rmcp::ErrorData::internal_error(
        format!("failed on {address}"),
        Some(serde_json::json!({ "url": address })),
    );
    let Err(error) = mask_response(Err(error), &|_: &str| None) else {
        panic!("still an error");
    };
    let rendered = serde_json::to_string(&error).expect("serializes");
    assert!(!rendered.contains(SECRET), "{rendered}");
}

/// A secret the engine knows has no name or shape to be found by: it is replaced word for word,
/// in prose, in JSON strings and keys, and in an error.
#[test]
fn a_known_secret_is_replaced_word_for_word() {
    const KNOWN: &str = "plain-looking-words";
    let known = |text: &str| {
        text.contains(KNOWN)
            .then(|| text.replace(KNOWN, "[redacted]"))
    };
    let mut object = serde_json::Map::new();
    object.insert("notes".to_owned(), format!("pw {KNOWN}").into());
    object.insert(KNOWN.to_owned(), 1.into());
    let json = serde_json::Value::Object(object).to_string();
    for text in [format!("$ echo {KNOWN}"), json] {
        let answer = Ok(CallToolResult::success(vec![ContentBlock::text(text)]).into());
        let masked = text_of(mask_response(answer, &known));
        assert!(!masked.contains(KNOWN), "{masked}");
        assert!(masked.contains("[redacted]"), "{masked}");
    }
    let error = rmcp::ErrorData::internal_error(format!("failed with {KNOWN}"), None);
    let Err(error) = mask_response(Err(error), &known) else {
        panic!("an error");
    };
    assert!(!error.message.contains(KNOWN), "{}", error.message);
    let untouched = Ok(CallToolResult::success(vec![ContentBlock::text("{\"a\":1}")]).into());
    assert_eq!(text_of(mask_response(untouched, &known)), "{\"a\":1}");
}

/// A key block in any answer - a terminal screen, a file window - loses its body.
#[test]
fn a_private_key_block_in_an_answer_is_masked() {
    let begin = format!("-----BEGIN RSA {}-----", "PRIVATE KEY");
    let end = format!("-----END RSA {}-----", "PRIVATE KEY");
    let screen = format!("$ cat id_rsa\n{begin}\nMIIEowIBAAKCAQEA{SECRET}\n{end}\n$ ");
    let mut answer = serde_json::json!({ "screen": screen });
    assert!(mask_value(&mut answer));
    let rendered = answer["screen"].as_str().expect("a string");
    assert!(!rendered.contains(SECRET), "{rendered}");
    assert_eq!(rendered, "$ cat id_rsa\n[private key redacted]\n$ ");
    // Masked once is masked for good.
    let again = answer.clone();
    assert!(!mask_value(&mut answer));
    assert_eq!(answer, again);
}

/// RC-080-12 (M11): an answer of a shape the mask does not walk never leaves as it is.
#[test]
fn an_answer_the_mask_cannot_walk_fails_closed() {
    let nothing = |_: &str| None;
    let other = InputRequiredResult::from_request_state(format!("password={SECRET}"));
    let answer = mask_response(Ok(CallToolResponse::InputRequired(other)), &nothing);
    let Ok(CallToolResponse::Complete(result)) = &answer else {
        panic!("a complete answer: {answer:?}");
    };
    assert_eq!(result.is_error, Some(true));
    let text = text_of(answer);
    assert!(text.contains("answer_unmaskable"), "{text}");
    assert!(!text.contains(SECRET), "{text}");
}

/// An address alone is seen whole, as before: its credentials go, and one with nothing to hide
/// stays as it was written, not as the parser writes it.
#[test]
fn an_address_alone_is_masked_as_an_address() {
    assert_eq!(
        redact_string(&format!("sftp://bob:{SECRET}@host.example/home")).as_deref(),
        Some("sftp://redacted@host.example/home")
    );
    // A scheme the prose rules do not search for.
    let other = redact_string(&format!("postgres://bob:{SECRET}@db.example/app"));
    assert_eq!(other.as_deref(), Some("postgres://redacted@db.example/app"));
    assert_eq!(redact_string("https://Example.COM"), None);
    assert_eq!(redact_string("https://api.example/v1/items"), None);
}

/// RC-110-20: a text whose first line is an address is no address. The parser drops line breaks
/// and tabs and encodes spaces, so each of these parses as one long address with a host and
/// nothing to hide - and left unmasked before the fix.
#[test]
fn text_that_starts_with_an_address_is_masked_as_text() {
    const FIRST: &str = "https://api.example/v1/items";
    let begin = format!("-----BEGIN RSA {}-----", "PRIVATE KEY");
    let end = format!("-----END RSA {}-----", "PRIVATE KEY");
    // A token known by its shape alone, of the token alphabet throughout.
    let prefixed = "ghp_0123456789abcdefghijklmnopqrstuv";
    for rest in [
        format!("\nAuthorization: Bearer {SECRET}\n"),
        format!("\r\nAuthorization: Bearer {SECRET}\r\n"),
        format!("\nsent with Bearer {SECRET} today"),
        format!("\npassword={SECRET}"),
        format!("\tpassword={SECRET}"),
        format!(" password={SECRET}"),
        format!("\nthe token is {prefixed} for now"),
        format!("\n{begin}\nMIIEowIBAAKCAQEA{SECRET}\n{end}\n"),
    ] {
        let text = format!("{FIRST}{rest}");
        // The premise, measured: the parser takes the whole text as one address with a host.
        let parsed = url::Url::parse(&text).expect("parses as one address");
        assert!(parsed.has_host(), "{text:?}");
        let masked = redact_string(&text).unwrap_or_else(|| panic!("unmasked: {text:?}"));
        assert!(!masked.contains(SECRET), "{masked:?}");
        assert!(!masked.contains(prefixed), "{masked:?}");
        assert!(masked.starts_with(FIRST), "{masked:?}");
    }
}

/// The same through the whole mask, as `files_read_text` answers a `.http` file: in a JSON
/// field and as a bare text block.
#[test]
fn a_file_that_starts_with_an_address_is_masked_in_an_answer() {
    let file = format!(
        "https://api.example/v1/items\nAuthorization: Bearer {SECRET}\npassword={SECRET}\n"
    );
    let mut answer = serde_json::json!({ "text": file });
    assert!(mask_value(&mut answer));
    let rendered = answer.to_string();
    assert!(!rendered.contains(SECRET), "{rendered}");
    let bare = Ok(CallToolResult::success(vec![ContentBlock::text(file)]).into());
    let text = text_of(mask_response(bare, &|_: &str| None));
    assert!(!text.contains(SECRET), "{text}");
    assert!(text.starts_with("https://api.example/v1/items\n"), "{text}");
}

/// An address with credentials inside a text loses them where it stands, in the first line and
/// in a later one, and the lines around it are masked as text.
#[test]
fn an_address_with_credentials_inside_a_text_is_masked() {
    let text = format!(
        "sftp://bob:{SECRET}@host.example/home\npassword={SECRET}\nmirror ftp://eve:{SECRET}@files.example/pub ok\n"
    );
    let masked = redact_string(&text).expect("masked");
    assert!(!masked.contains(SECRET), "{masked}");
    assert_eq!(
        masked,
        "sftp://redacted@host.example/home\npassword=[redacted]\nmirror ftp://redacted@files.example/pub ok\n"
    );
    // Masked once is masked for good.
    assert_eq!(redact_string(&masked), None);
}

/// RC-110-20: an address of a scheme the text rules do not search keeps no password in a text
/// either - alone, before further lines, inside a line of prose, twice in one text - and the
/// white space around it stays as it was.
#[test]
fn an_address_of_any_scheme_is_masked_inside_a_text() {
    const MASKED: &str = "postgres://redacted@host/db";
    let address = format!("postgres://user:{SECRET}@host/db");
    for (text, want) in [
        (address.clone(), MASKED.to_owned()),
        (
            format!("{address}\nsslmode=require\r\n\tpool: 4\n"),
            format!("{MASKED}\nsslmode=require\r\n\tpool: 4\n"),
        ),
        (
            format!("connect to  {address} and wait"),
            format!("connect to  {MASKED} and wait"),
        ),
        (
            format!("DATABASE_URL=\"{address}\"\n\n  replica:\t(redis://:{SECRET}@cache:6379/0)\n"),
            format!("DATABASE_URL=\"{MASKED}\"\n\n  replica:\t(redis://redacted@cache:6379/0)\n"),
        ),
    ] {
        let masked = redact_string(&text).unwrap_or_else(|| panic!("unmasked: {text:?}"));
        assert!(!masked.contains(SECRET), "{masked:?}");
        assert_eq!(masked, want);
        // Masked once is masked for good.
        assert_eq!(redact_string(&masked), None, "{masked:?}");
    }
    // Nothing to hide: byte for byte, so `None`.
    for text in [
        "postgres://host/db\nsslmode=require\n",
        "see  postgres://host:5432/db\tand (amqp://broker/vhost),\r\nthen a::b://c",
        "mail bob@example.com about x://",
    ] {
        assert_eq!(redact_string(text), None, "{text:?}");
    }
}
