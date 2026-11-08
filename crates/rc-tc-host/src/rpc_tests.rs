use serde_json::json;

use super::*;

#[test]
fn messages_serialise_as_json_rpc_2() {
    let request = Message::request(3, method::WCX_LIST, json!({ "archive": "C:\\a.7z" }));
    let value: Value = serde_json::from_slice(&request.to_json()).expect("json");
    assert_eq!(
        value,
        json!({ "jsonrpc": "2.0", "id": 3, "method": "wcx.list", "params": { "archive": "C:\\a.7z" } })
    );
    let note = Message::notification(method::CREDIT, json!({ "stream": 1, "bytes": 5 }));
    let value: Value = serde_json::from_slice(&note.to_json()).expect("json");
    assert!(value.get("id").is_none());
    let failed = Message::failure(4, code::PLUGIN, "E_BAD_ARCHIVE");
    let value: Value = serde_json::from_slice(&failed.to_json()).expect("json");
    assert_eq!(
        value["error"],
        json!({ "code": 2, "message": "E_BAD_ARCHIVE" })
    );
}

#[test]
fn messages_read_back_and_other_versions_are_refused() {
    let answer = Message::answer(9, json!({ "size": 12 }));
    assert_eq!(Message::from_json(&answer.to_json()), Some(answer));
    assert_eq!(Message::from_json(br#"{"jsonrpc":"1.0","id":1}"#), None);
    assert_eq!(Message::from_json(b"not json"), None);
    assert_eq!(Message::from_json(br#"{"jsonrpc":"2.0","id":-1}"#), None);
}

#[test]
fn kinds_are_lowercase_names() {
    assert_eq!(
        serde_json::to_value([Kind::Wcx, Kind::Wlx, Kind::Wfx, Kind::Wdx]).expect("json"),
        json!(["wcx", "wlx", "wfx", "wdx"])
    );
}
