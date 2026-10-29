use rmcp::model::{CallToolResponse, CallToolResult, ContentBlock, InputRequiredResult};

use super::{mask_response, mask_value};

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
