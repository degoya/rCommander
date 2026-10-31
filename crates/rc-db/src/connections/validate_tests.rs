use super::*;
use crate::{
    SecretFlags,
    connections::{ConnectionId, ConnectionKind},
};

#[test]
fn a_name_is_trimmed_and_must_not_be_blank() {
    assert_eq!(name("  web  ").expect("name"), "web");
    assert!(matches!(name(""), Err(ConnectionError::NameEmpty)));
    assert!(matches!(name(" \t "), Err(ConnectionError::NameEmpty)));
}

#[test]
fn name_user_and_start_directory_refuse_control_characters() {
    let refused = |result: Result<Fields, ConnectionError>, wanted: &str| match result {
        Err(ConnectionError::ControlCharacter { field }) => assert_eq!(field, wanted),
        other => panic!("{wanted}: {other:?}"),
    };
    let draft = |name: &str, user: &str, start: &str| ConnectionDraft {
        username: Some(user.into()),
        start_directory: Some(start.into()),
        ..ConnectionDraft::new(name, "example.org")
    };
    assert!(Fields::from_draft(&draft("web", "deploy", "/srv/my site")).is_ok());
    refused(
        Fields::from_draft(&draft("we\u{1b}b", "deploy", "/srv")),
        "name",
    );
    refused(
        Fields::from_draft(&draft("web", "de\nploy", "/srv")),
        "user name",
    );
    refused(
        Fields::from_draft(&draft("web", "deploy", "/srv\r\nrm -rf ~")),
        "start directory",
    );
    refused(
        Fields::from_draft(&draft("web", "deploy", "/srv\0x")),
        "start directory",
    );
    let current = Connection {
        start_directory: Some("/srv\nwhoami".into()),
        ..stored()
    };
    let patch = ConnectionPatch {
        notes: Some("multi\nline notes are fine".into()),
        ..ConnectionPatch::default()
    };
    refused(Fields::patched(&current, &patch), "start directory");
}

#[test]
fn a_host_is_trimmed_and_must_be_one_word() {
    assert_eq!(host(" example.org ").expect("host"), "example.org");
    assert_eq!(host("::1").expect("ipv6"), "::1");
    assert!(matches!(host(""), Err(ConnectionError::HostEmpty)));
    assert!(matches!(host("   "), Err(ConnectionError::HostEmpty)));
    assert!(matches!(host("a b"), Err(ConnectionError::HostInvalid)));
    assert!(matches!(host("a\nb"), Err(ConnectionError::HostInvalid)));
}

#[test]
fn a_port_is_one_to_65535() {
    assert_eq!(port(1).expect("1"), 1);
    assert_eq!(port(65535).expect("65535"), 65535);
    for bad in [0, -1, 65536, i64::MAX] {
        assert!(
            matches!(port(bad), Err(ConnectionError::PortInvalid { port }) if port == bad),
            "{bad}"
        );
    }
}

#[test]
fn optional_text_that_is_blank_is_none() {
    assert_eq!(optional(None), None);
    assert_eq!(optional(Some("")), None);
    assert_eq!(optional(Some("  ")), None);
    assert_eq!(optional(Some(" root ")).as_deref(), Some("root"));
}

#[test]
fn a_draft_without_a_port_takes_the_kind_default() {
    let fields = Fields::from_draft(&ConnectionDraft::new("web", "example.org")).expect("valid");
    assert_eq!(fields.port, ConnectionKind::Ssh.default_port());
    assert_eq!(fields.auth_method, AuthMethod::Password);
    assert_eq!(fields.default_view, DefaultView::Files);
}

#[test]
fn a_draft_reports_the_first_invalid_field() {
    let mut draft = ConnectionDraft::new("web", "");
    assert!(matches!(
        Fields::from_draft(&draft),
        Err(ConnectionError::HostEmpty)
    ));
    draft.host = "example.org".into();
    draft.port = Some(0);
    assert!(matches!(
        Fields::from_draft(&draft),
        Err(ConnectionError::PortInvalid { port: 0 })
    ));
}

fn stored() -> Connection {
    Connection {
        id: ConnectionId(1),
        folder_id: None,
        name: "web".into(),
        kind: ConnectionKind::Ssh,
        host: "example.org".into(),
        port: 2222,
        username: Some("deploy".into()),
        auth_method: AuthMethod::Key,
        key_path: Some("C:\\keys\\id_ed25519".into()),
        login_script_id: None,
        default_view: DefaultView::Terminal,
        start_directory: Some("/srv".into()),
        notes: Some("blue rack".into()),
        ftp_data_mode: None,
        ftp_encoding: None,
        theme_light: None,
        theme_dark: None,
        sort_order: 0,
        created_at: String::new(),
        updated_at: String::new(),
        secrets: SecretFlags::default(),
        key: None,
        credential_id: None,
    }
}

#[test]
fn a_patch_keeps_what_it_does_not_name() {
    let current = stored();
    let fields = Fields::patched(
        &current,
        &ConnectionPatch {
            host: Some("example.net".into()),
            ..ConnectionPatch::default()
        },
    )
    .expect("valid");
    assert_eq!(fields.host, "example.net");
    assert_eq!(fields.port, 2222);
    assert_eq!(fields.username.as_deref(), Some("deploy"));
    assert_eq!(fields.default_view, DefaultView::Terminal);
}

#[test]
fn an_empty_string_in_a_patch_clears_optional_text() {
    let fields = Fields::patched(
        &stored(),
        &ConnectionPatch {
            username: Some(String::new()),
            key_path: Some("  ".into()),
            ..ConnectionPatch::default()
        },
    )
    .expect("valid");
    assert_eq!(fields.username, None);
    assert_eq!(fields.key_path, None);
    assert_eq!(fields.notes.as_deref(), Some("blue rack"));
}

#[test]
fn a_patch_is_validated_like_a_draft() {
    let current = stored();
    let blank = ConnectionPatch {
        name: Some(" ".into()),
        ..ConnectionPatch::default()
    };
    assert!(matches!(
        Fields::patched(&current, &blank),
        Err(ConnectionError::NameEmpty)
    ));
    let port = ConnectionPatch {
        port: Some(70000),
        ..ConnectionPatch::default()
    };
    assert!(matches!(
        Fields::patched(&current, &port),
        Err(ConnectionError::PortInvalid { port: 70000 })
    ));
}

#[test]
fn an_ftp_draft_takes_its_kinds_port() {
    for (kind, port) in [
        (ConnectionKind::Ftp, 21),
        (ConnectionKind::Ftps, 21),
        (ConnectionKind::FtpsImplicit, 990),
    ] {
        let draft = ConnectionDraft {
            kind,
            auth_method: AuthMethod::Anonymous,
            ..ConnectionDraft::new("nas", "nas.local")
        };
        assert_eq!(Fields::from_draft(&draft).expect("valid").port, port);
    }
}

#[test]
fn a_kind_logs_in_only_with_its_auth_methods() {
    for kind in ConnectionKind::ALL {
        for method in AuthMethod::ALL {
            let draft = ConnectionDraft {
                kind: *kind,
                auth_method: *method,
                username: Some("web".into()),
                ..ConnectionDraft::new("x", "example.org")
            };
            let ftp_ok = matches!(method, AuthMethod::Password | AuthMethod::Anonymous);
            let expected = if kind.is_ftp() {
                ftp_ok
            } else {
                *method != AuthMethod::Anonymous
            };
            let result = Fields::from_draft(&draft);
            assert_eq!(result.is_ok(), expected, "{kind:?} with {method:?}");
            if !expected {
                assert!(
                    matches!(result, Err(ConnectionError::AuthUnsupported { kind: k, auth_method: m }) if k == *kind && m == *method)
                );
            }
        }
    }
}

#[test]
fn ftp_with_a_password_needs_a_user_and_anonymous_does_not() {
    let mut draft = ConnectionDraft {
        kind: ConnectionKind::Ftp,
        username: Some("  ".into()),
        ..ConnectionDraft::new("nas", "nas.local")
    };
    assert!(matches!(
        Fields::from_draft(&draft),
        Err(ConnectionError::UsernameEmpty)
    ));
    draft.auth_method = AuthMethod::Anonymous;
    assert!(Fields::from_draft(&draft).is_ok());
    // SSH may leave the user to the server's or the system's default.
    draft.kind = ConnectionKind::Ssh;
    draft.auth_method = AuthMethod::Password;
    assert!(Fields::from_draft(&draft).is_ok());
}

#[test]
fn ftp_opens_only_as_files() {
    let draft = ConnectionDraft {
        kind: ConnectionKind::Ftps,
        default_view: DefaultView::Terminal,
        username: Some("web".into()),
        ..ConnectionDraft::new("nas", "nas.local")
    };
    assert!(matches!(
        Fields::from_draft(&draft),
        Err(ConnectionError::ViewUnsupported {
            kind: ConnectionKind::Ftps
        })
    ));
}

#[test]
fn a_patch_to_ftp_checks_the_kept_fields() {
    // The stored SSH connection logs in with a key and opens a terminal: neither fits FTP.
    let to_ftp = ConnectionPatch {
        kind: Some(ConnectionKind::Ftp),
        ..ConnectionPatch::default()
    };
    assert!(matches!(
        Fields::patched(&stored(), &to_ftp),
        Err(ConnectionError::AuthUnsupported { .. })
    ));
    let whole = ConnectionPatch {
        kind: Some(ConnectionKind::Ftp),
        auth_method: Some(AuthMethod::Anonymous),
        default_view: Some(DefaultView::Files),
        ..ConnectionPatch::default()
    };
    let fields = Fields::patched(&stored(), &whole).expect("valid");
    assert_eq!(fields.kind, ConnectionKind::Ftp);
    assert_eq!(fields.port, 2222, "a patch keeps the port it does not name");
}

#[test]
fn the_ftp_options_are_kept_for_ftp_and_dropped_for_ssh() {
    let draft = ConnectionDraft {
        kind: ConnectionKind::Ftp,
        auth_method: AuthMethod::Anonymous,
        ftp_data_mode: Some(FtpDataMode::Active),
        ftp_encoding: Some(FtpEncoding::Utf8),
        ..ConnectionDraft::new("nas", "nas.local")
    };
    let fields = Fields::from_draft(&draft).expect("valid");
    assert_eq!(fields.ftp_data_mode, Some(FtpDataMode::Active));
    assert_eq!(fields.ftp_encoding, Some(FtpEncoding::Utf8));
    let ssh = ConnectionDraft {
        kind: ConnectionKind::Ssh,
        auth_method: AuthMethod::Password,
        ..draft
    };
    let fields = Fields::from_draft(&ssh).expect("valid");
    assert_eq!((fields.ftp_data_mode, fields.ftp_encoding), (None, None));
}

/// The code, field and limit of a length refusal.
fn too_long(result: Result<Fields, ConnectionError>) -> (&'static str, &'static str, usize) {
    let error = result.expect_err("too long");
    let code = error.code();
    match error {
        ConnectionError::TextTooLong { field, max }
        | ConnectionError::PathTooLong { field, max } => (code, field, max),
        other => panic!("too long: {other:?}"),
    }
}

/// RC-080-47: a name, host or user name is at most 255 characters (`text_too_long`), a key path
/// or start directory at most 4096 (`path_too_long`), counted in characters; a draft and a
/// patch are held to it alike, and a folder or credential name through [`name`].
#[test]
fn text_and_paths_are_held_to_their_length() {
    let at = |max: usize| "\u{e9}".repeat(max);
    let over = |max: usize| "x".repeat(max + 1);
    let fits = ConnectionDraft {
        username: Some(at(MAX_TEXT_CHARS)),
        key_path: Some(at(MAX_PATH_CHARS)),
        start_directory: Some(at(MAX_PATH_CHARS)),
        ..ConnectionDraft::new(at(MAX_TEXT_CHARS), at(MAX_TEXT_CHARS))
    };
    assert!(
        Fields::from_draft(&fits).is_ok(),
        "the limits themselves fit"
    );
    let text = |field| ("text_too_long", field, MAX_TEXT_CHARS);
    let path = |field| ("path_too_long", field, MAX_PATH_CHARS);
    let cases = [
        (
            ConnectionDraft {
                name: over(MAX_TEXT_CHARS),
                ..fits.clone()
            },
            text("name"),
        ),
        (
            ConnectionDraft {
                host: over(MAX_TEXT_CHARS),
                ..fits.clone()
            },
            text("host"),
        ),
        (
            ConnectionDraft {
                username: Some(over(MAX_TEXT_CHARS)),
                ..fits.clone()
            },
            text("user name"),
        ),
        (
            ConnectionDraft {
                key_path: Some(over(MAX_PATH_CHARS)),
                ..fits.clone()
            },
            path("key path"),
        ),
        (
            ConnectionDraft {
                start_directory: Some(over(MAX_PATH_CHARS)),
                ..fits.clone()
            },
            path("start directory"),
        ),
    ];
    for (draft, refused) in cases {
        assert_eq!(too_long(Fields::from_draft(&draft)), refused);
    }
    let patch = ConnectionPatch {
        key_path: Some(over(MAX_PATH_CHARS)),
        ..ConnectionPatch::default()
    };
    assert_eq!(
        too_long(Fields::patched(&stored(), &patch)),
        path("key path")
    );
    let patch = ConnectionPatch {
        username: Some(over(MAX_TEXT_CHARS)),
        ..ConnectionPatch::default()
    };
    assert_eq!(
        too_long(Fields::patched(&stored(), &patch)),
        text("user name")
    );
    assert!(matches!(
        name(&over(MAX_TEXT_CHARS)),
        Err(ConnectionError::TextTooLong { field: "name", .. })
    ));
    assert!(matches!(
        user(Some(&over(MAX_TEXT_CHARS))),
        Err(ConnectionError::TextTooLong {
            field: "user name",
            ..
        })
    ));
}
