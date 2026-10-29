use std::path::{Path, PathBuf};

use rc_db::connections::{ImportResult, ImportStatus};

use super::{candidates, key_of, report, source_of};
use crate::{
    AuthMethod,
    import::ssh_config::{
        ImportWarning, ImportedHost, SkipReason, SkippedHost, SourceLine, SshConfigImport,
        WarningKind,
    },
};

fn at(line: usize) -> SourceLine {
    SourceLine {
        file: PathBuf::from("/home/tester/.ssh/config"),
        line,
    }
}

fn host(alias: &str) -> ImportedHost {
    ImportedHost {
        alias: alias.to_owned(),
        host_name: None,
        user: None,
        port: None,
        identity_files: Vec::new(),
        proxy_jump: None,
        ignored_options: Vec::new(),
        source: at(1),
    }
}

fn found(hosts: Vec<ImportedHost>) -> SshConfigImport {
    SshConfigImport {
        hosts,
        ..SshConfigImport::default()
    }
}

#[test]
fn a_host_becomes_a_draft_with_its_key_as_a_path() {
    let db = ImportedHost {
        host_name: Some("10.0.0.5".into()),
        user: Some("deploy".into()),
        port: Some(2222),
        identity_files: vec![
            PathBuf::from("/home/tester/.ssh/id_db"),
            PathBuf::from("/home/tester/keys/other"),
        ],
        proxy_jump: Some("bastion".into()),
        ..host("DB")
    };
    let made = candidates(&found(vec![db, host("plain")]), None);
    assert_eq!(made[0].key, "db");
    assert!(made[0].selected);
    let draft = &made[0].draft;
    assert_eq!(draft.name, "DB");
    assert_eq!(draft.host, "10.0.0.5");
    assert_eq!(draft.port, Some(2222));
    assert_eq!(draft.username.as_deref(), Some("deploy"));
    assert_eq!(draft.auth_method, AuthMethod::Key);
    assert_eq!(draft.key_path.as_deref(), Some("/home/tester/.ssh/id_db"));
    let notes = draft.notes.as_deref().expect("notes");
    assert!(notes.contains("ProxyJump bastion"), "{notes}");
    assert!(notes.contains("/home/tester/keys/other"), "{notes}");

    let plain = &made[1].draft;
    assert_eq!(
        plain.host, "plain",
        "the alias is the address without HostName"
    );
    assert_eq!(plain.auth_method, AuthMethod::Agent);
    assert_eq!(plain.key_path, None);
    assert_eq!(plain.notes, None);
}

#[test]
fn the_selection_names_aliases_ignoring_case() {
    let chosen = ["WEB".to_owned()];
    let made = candidates(&found(vec![host("web"), host("db")]), Some(&chosen));
    assert!(made[0].selected);
    assert!(!made[1].selected);
}

#[test]
fn keys_and_sources_are_stable_strings() {
    assert_eq!(key_of("Mixed"), "mixed");
    assert_eq!(
        source_of(Path::new("/home/tester/.ssh/config")),
        "ssh_config:/home/tester/.ssh/config"
    );
}

#[test]
fn the_report_carries_codes_and_details_never_other_values() {
    let mut parsed = found(vec![ImportedHost {
        ignored_options: vec!["ForwardAgent".into()],
        ..host("web")
    }]);
    parsed.skipped.push(SkippedHost {
        name: "*".into(),
        reason: SkipReason::Pattern,
        source: at(4),
    });
    parsed.warnings.push(ImportWarning {
        source: at(9),
        kind: WarningKind::InvalidPort {
            value: "http".into(),
        },
    });
    parsed.warnings.push(ImportWarning {
        source: at(12),
        kind: WarningKind::MatchSkipped,
    });
    let made = candidates(&parsed, None);
    let shown = report(
        "/home/tester/.ssh/config".into(),
        true,
        &parsed,
        &made,
        ImportResult {
            folder: None,
            folder_created: false,
            statuses: vec![ImportStatus::New],
        },
    );
    assert_eq!(shown.hosts.len(), 1);
    assert_eq!(shown.hosts[0].status, ImportStatus::New);
    assert_eq!(shown.hosts[0].ignored_options, ["ForwardAgent"]);
    assert_eq!(shown.skipped[0].reason, "pattern");
    assert_eq!(shown.warnings[0].code, "invalid_port");
    assert_eq!(
        shown.warnings[0].keyword.as_deref(),
        Some("Port"),
        "never the value"
    );
    assert_eq!(shown.warnings[1].code, "match_skipped");
    assert_eq!(shown.warnings[1].keyword.as_deref(), Some("Match"));

    let json = serde_json::to_value(&shown.hosts[0]).expect("json");
    assert_eq!(json["status"], "new", "the status is flattened: {json}");
    assert!(
        json.get("key").is_none(),
        "no field the MCP mask would blank"
    );
}

/// Text of a file that is no configuration - a key, a document - never comes back through a
/// warning or an ignored option: only recognised or plausible keywords and line numbers do.
#[test]
fn raw_text_of_the_file_is_never_echoed() {
    let blob = "b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQ==";
    let mut parsed = found(vec![ImportedHost {
        ignored_options: vec![
            "ForwardAgent".into(),
            blob.into(),
            "sk-ant-api03".into(),
            "A".repeat(40),
        ],
        ..host("web")
    }]);
    for kind in [
        WarningKind::OptionOutsideHost {
            keyword: blob.into(),
        },
        WarningKind::OptionOutsideHost {
            keyword: "ServerAliveInterval".into(),
        },
        WarningKind::MissingArgument {
            keyword: "hunter2!".into(),
        },
        WarningKind::UnknownToken { token: "%Q".into() },
        WarningKind::IncludeNotFound {
            file: PathBuf::from("/home/tester/Documents/private.txt"),
        },
    ] {
        parsed.warnings.push(ImportWarning {
            source: at(7),
            kind,
        });
    }
    let made = candidates(&parsed, None);
    let shown = report(
        "/home/tester/.ssh/config".into(),
        true,
        &parsed,
        &made,
        ImportResult {
            folder: None,
            folder_created: false,
            statuses: vec![ImportStatus::New],
        },
    );
    assert_eq!(
        shown.hosts[0].ignored_options,
        ["ForwardAgent".to_owned(), "A".repeat(32)]
    );
    let keywords: Vec<Option<&str>> = shown
        .warnings
        .iter()
        .map(|warning| warning.keyword.as_deref())
        .collect();
    assert_eq!(
        keywords,
        [
            None,
            Some("ServerAliveInterval"),
            None,
            None,
            Some("Include")
        ]
    );
    assert!(shown.warnings.iter().all(|warning| warning.line == 7));
    let json = serde_json::to_string(&shown).expect("json");
    for leaked in [blob, "sk-ant", "hunter2", "%Q", "private.txt"] {
        assert!(!json.contains(leaked), "{leaked} leaked: {json}");
    }
}
