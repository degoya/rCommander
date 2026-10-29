use super::*;

fn plain(content: &str) -> String {
    format!(
        r#"{{"format":"rcommander-export","version":1,"created_at":"2026-10-18T10:00:00Z","content":{content}}}"#
    )
}

#[test]
fn hex_goes_both_ways() {
    let bytes = [0_u8, 1, 0x7f, 0x80, 0xff];
    assert_eq!(hex(&bytes), "00017f80ff");
    assert_eq!(unhex("00017F80ff").as_deref(), Some(&bytes[..]));
    assert_eq!(unhex("abc"), None);
    assert_eq!(unhex("zz"), None);
}

#[test]
fn a_plain_file_parses() {
    let text = plain(r#"{"origin":"abc123","secrets":false,"settings":[]}"#);
    let file = ExportFile::parse(text.as_bytes()).expect("parses");
    let content = file.content.expect("content");
    assert_eq!(content.origin, "abc123");
    assert!(content.check().is_ok());
}

#[test]
fn another_format_version_or_shape_is_refused() {
    let other =
        r#"{"format":"x","version":1,"created_at":"","content":{"origin":"a","secrets":false}}"#;
    assert!(matches!(
        ExportFile::parse(other.as_bytes()),
        Err(ExchangeError::FileInvalid { .. })
    ));
    let newer = r#"{"format":"rcommander-export","version":2,"created_at":""}"#;
    assert!(matches!(
        ExportFile::parse(newer.as_bytes()),
        Err(ExchangeError::VersionUnknown { version: 2 })
    ));
    let neither = r#"{"format":"rcommander-export","version":1,"created_at":""}"#;
    assert!(ExportFile::parse(neither.as_bytes()).is_err());
    let unknown = plain(r#"{"origin":"a","secrets":false,"mcp_token":"x"}"#);
    assert!(ExportFile::parse(unknown.as_bytes()).is_err());
    assert!(ExportFile::parse(b"not json").is_err());
}

#[test]
fn caps_hold_for_lists_and_texts() {
    let mut content = Content {
        origin: "abc".to_owned(),
        ..Content::default()
    };
    assert!(content.check().is_ok());
    content.settings = Some(vec![
        ExportedSetting {
            setting: "a".to_owned(),
            value: Value::Null,
        };
        MAX_ITEMS + 1
    ]);
    assert!(content.check().is_err());
    content.settings = Some(vec![ExportedSetting {
        setting: "x".repeat(MAX_TEXT + 1),
        value: Value::Null,
    }]);
    assert!(content.check().is_err());
    content.settings = None;
    content.origin = "../etc".to_owned();
    assert!(content.check().is_err());
}

#[test]
fn a_secret_never_shows_in_debug() {
    let connection = ExportedConnection {
        source: "rcexport:abc".to_owned(),
        key: "1".to_owned(),
        folders: Vec::new(),
        connection: ConnectionDraft::new("web", "example.org"),
        login_script: None,
        password: Some(ExportSecret::new("canary-hunter2")),
        private_key: None,
        passphrase: None,
        sources: Vec::new(),
    };
    assert!(connection.has_secrets());
    assert!(!format!("{connection:?}").contains("canary-hunter2"));
    let json = serde_json::to_string(&connection).expect("serializes");
    assert!(json.contains("canary-hunter2"));
}

#[test]
fn a_sealed_file_round_trips_its_bytes() {
    let sealed = SealedExport {
        kdf: "argon2id".to_owned(),
        params: KdfParams::DEFAULT,
        salt: vec![1; 16],
        sealed: vec![1, 2, 3, 250],
    };
    let file = SealedFile::of(&sealed);
    assert_eq!(file.content, "010203fa");
    assert_eq!(file.sealed().expect("reads"), sealed);
    let broken = SealedFile {
        content: "xyz".to_owned(),
        ..file
    };
    assert!(broken.sealed().is_err());
}
