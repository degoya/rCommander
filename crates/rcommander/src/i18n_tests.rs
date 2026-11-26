use super::{
    Catalogue, english, error_text, fill, fill_in, lookup, number, plural, plural_form, resolve,
};
use crate::locale::Lang;

#[test]
fn the_catalogue_parses_and_is_not_empty() {
    assert!(english().len() > 5, "locales/en.json did not parse");
}

#[test]
fn a_missing_key_shows_itself() {
    assert_eq!(lookup("no.such.key"), "no.such.key");
}

#[test]
fn placeholders_are_filled() {
    let text = fill("terminal.shell.wsl", &[("distro", "Debian".to_owned())]);
    assert_eq!(text, "WSL: Debian");
}

/// A key a translation lacks reads in English; a key neither has is `None`, and the caller
/// shows the key.
#[test]
fn a_key_missing_in_a_language_falls_back_to_english() {
    let german: Catalogue = [("column.name".to_owned(), "Name".to_owned())]
        .into_iter()
        .collect();
    assert_eq!(resolve("column.name", &german, english()), Some("Name"));
    assert_eq!(
        resolve("column.size", &german, english()),
        english().get("column.size").map(String::as_str)
    );
    assert_eq!(resolve("no.such.key", &german, english()), None);
}

/// An empty text in a translation is no text: the key reads in English (RC-100-47).
#[test]
fn an_empty_translation_falls_back_to_english() {
    let german: Catalogue = [("column.name".to_owned(), String::new())]
        .into_iter()
        .collect();
    assert_eq!(resolve("column.name", &german, english()), Some("Name"));
}

/// Every language's catalogue is compiled in and parses: one that did not would be empty, and
/// the whole interface would fall back to English without a word (RC-100-47).
#[test]
fn every_catalogue_is_compiled_in_and_parses() {
    for (lang, text) in [
        (Lang::De, include_str!("../locales/de.json")),
        (Lang::Fr, include_str!("../locales/fr.json")),
        (Lang::Es, include_str!("../locales/es.json")),
    ] {
        let parsed = serde_json::from_str::<Catalogue>(text);
        assert!(parsed.is_ok(), "the catalogue of {lang:?} does not parse");
    }
}

#[test]
fn an_error_code_reads_as_its_message_or_as_itself() {
    assert_eq!(error_text("permission_denied"), "Permission denied.");
    assert_eq!(error_text("no_such_code"), "no_such_code");
}

/// A drive that does not answer and an MCP question nobody answered each show their own text:
/// both codes once read `timed_out`, and the window showed one text for both (RC-080-12).
#[test]
fn a_hanging_drive_and_an_unanswered_question_read_differently() {
    let drive = rc_vfs::VfsError::TimedOut {
        path: "/share".into(),
        seconds: 20,
    };
    let drive_text = error_text(drive.code());
    assert!(drive_text.contains("drive or share"), "{drive_text}");
    assert!(rc_mcp::ALL_CODES.contains(&"timed_out"));
    let question_text = error_text("timed_out");
    assert!(question_text.contains("question"), "{question_text}");
    assert_ne!(drive_text, question_text);
}

/// Every stable error code - the file system's, the engine's, the terminal's, the connection
/// tree's, the vault's, the host-key check's, the operations', the backup's, multi-rename's, the
/// SSH agents', the checksum runs', FTP's, SSH sessions', SFTP's, login scripts', the viewer
/// search's, the plugin commands' and the MCP server's own - has a message under `error.<code>`. A code two of them
/// share has one key.
#[test]
fn every_error_code_has_a_message() {
    let codes = rc_vfs::VfsError::ALL_CODES
        .iter()
        .chain(rc_engine::EngineError::ALL_CODES)
        .chain(rc_engine::TabsError::ALL_CODES)
        .chain(rc_engine::TermError::ALL_CODES)
        .chain(rc_engine::ConnectionError::ALL_CODES)
        .chain(rc_vault::VaultError::ALL_CODES)
        .chain(rc_remote::KeyError::ALL_CODES)
        .chain(rc_ops::OpsError::ALL_CODES)
        .chain(rc_db::BackupError::ALL_CODES)
        .chain(rc_rename::RenameError::ALL_CODES)
        .chain(rc_rename::Conflict::ALL_CODES)
        .chain(rc_rename::InvalidName::ALL_CODES)
        .chain(rc_remote::AgentError::ALL_CODES)
        .chain(rc_engine::ChecksumError::ALL_CODES)
        .chain(rc_remote::FtpError::ALL_CODES)
        .chain(
            rc_remote::ftp::UNSUPPORTED_COMMANDS
                .iter()
                .map(|(_, code)| code),
        )
        .chain(rc_engine::CertificateError::ALL_CODES)
        .chain(rc_engine::NetworkDriveError::ALL_CODES)
        .chain(rc_engine::PluginAdminError::ALL_CODES)
        .chain(rc_engine::WorkspaceError::ALL_CODES)
        .chain(rc_engine::ColumnSetError::ALL_CODES)
        .chain(rc_engine::UserCommandError::ALL_CODES)
        .chain(rc_engine::RemoteEditError::ALL_CODES)
        .chain(rc_engine::ButtonBarError::ALL_CODES)
        .chain(rc_engine::inventory::InventorySourceError::ALL_CODES)
        .chain(rc_engine::ForwardError::ALL_CODES)
        .chain(rc_db::inventory::InventoryError::ALL_CODES)
        .chain(rc_remote::SshError::ALL_CODES)
        .chain(rc_remote::SftpError::ALL_CODES)
        .chain(rc_engine::viewer::PatternError::ALL_CODES)
        .chain(rc_engine::LoginScriptError::ALL_CODES)
        .chain(rc_engine::ScriptInvalid::ALL_CODES)
        .chain(rc_remote::login_script::ScriptError::ALL_CODES)
        .chain(rc_mcp::ALL_CODES)
        // A server that could not start (`mcp.rs`, RC-110-86).
        .chain(&[
            "mcp_token_unavailable",
            "mcp_port_in_use",
            "mcp_start_failed",
        ])
        .chain(rc_engine::plugins::ALL_CODES)
        .chain(rc_engine::settings::themes::ThemeFileError::ALL_CODES)
        .chain(&["theme_file_unreadable", "theme_file_unwritable"]);
    let missing: Vec<String> = codes
        .map(|code| format!("error.{code}"))
        .filter(|key| !english().contains_key(key))
        .collect();
    assert!(missing.is_empty(), "keys missing in en.json: {missing:?}");
}

/// Every stable code of the ssh_config import - status, skip reason, warning - has a text
/// under `ssh_import.<kind>.<code>`.
#[test]
fn every_ssh_import_code_has_a_text() {
    use rc_engine::import::ssh_config::{SkipReason, WarningKind};
    let statuses = [
        "new",
        "created",
        "update",
        "updated",
        "unchanged",
        "refused",
    ];
    let keys = statuses
        .iter()
        .map(|code| format!("ssh_import.status.{code}"))
        .chain(
            SkipReason::ALL_CODES
                .iter()
                .map(|code| format!("ssh_import.skip.{code}")),
        )
        .chain(
            WarningKind::ALL_CODES
                .iter()
                .map(|code| format!("ssh_import.warning.{code}")),
        );
    let missing: Vec<String> = keys.filter(|key| !english().contains_key(key)).collect();
    assert!(missing.is_empty(), "keys missing in en.json: {missing:?}");
}

/// The plural rule of each language (RC-110-82): 1 alone is `one` in English, German and
/// Spanish, 0 and 1 are in French; a count that is no number is `other`.
#[test]
fn each_language_has_its_plural_rule() {
    for (lang, zero, one, two) in [
        (Lang::En, "other", "one", "other"),
        (Lang::De, "other", "one", "other"),
        (Lang::Fr, "one", "one", "other"),
        (Lang::Es, "other", "one", "other"),
    ] {
        assert_eq!(plural_form(lang, Some(0)), zero, "{lang:?} 0");
        assert_eq!(plural_form(lang, Some(1)), one, "{lang:?} 1");
        assert_eq!(plural_form(lang, Some(2)), two, "{lang:?} 2");
        assert_eq!(plural_form(lang, None), "other", "{lang:?} no number");
    }
    assert_eq!(number("1"), Some(1));
    assert_eq!(number("1\u{202f}234"), Some(1234));
    // Every language's grouping of `format::count` (RC-110-83).
    assert_eq!(number("1,234,567"), Some(1_234_567));
    assert_eq!(number("1.234.567"), Some(1_234_567));
    assert_eq!(number("1\u{202f}234\u{202f}567"), Some(1_234_567));
    assert_eq!(number(&crate::format::count(1_000_001)), Some(1_000_001));
    assert_eq!(number("many"), None);
}

/// Three counted texts at 0, 1 and 2 in every language read as the language says them: "1 job
/// selected", not "1 jobs selected"; French puts 0 in the singular, Spanish not (RC-110-82).
#[test]
fn counted_texts_take_the_plural_form_of_their_count() {
    let cases: [(Lang, &str, [&str; 3]); 12] = [
        (
            Lang::En,
            "queue.marked",
            ["0 jobs selected", "1 job selected", "2 jobs selected"],
        ),
        (
            Lang::De,
            "queue.marked",
            [
                "0 Auftr\u{e4}ge markiert",
                "1 Auftrag markiert",
                "2 Auftr\u{e4}ge markiert",
            ],
        ),
        (
            Lang::Fr,
            "queue.marked",
            [
                "0 t\u{e2}che s\u{e9}lectionn\u{e9}e",
                "1 t\u{e2}che s\u{e9}lectionn\u{e9}e",
                "2 t\u{e2}ches s\u{e9}lectionn\u{e9}es",
            ],
        ),
        (
            Lang::Es,
            "queue.marked",
            [
                "0 trabajos seleccionados",
                "1 trabajo seleccionado",
                "2 trabajos seleccionados",
            ],
        ),
        (
            Lang::En,
            "ops.background",
            [
                "0 jobs in the background",
                "1 job in the background",
                "2 jobs in the background",
            ],
        ),
        (
            Lang::De,
            "ops.background",
            [
                "0 Auftr\u{e4}ge im Hintergrund",
                "1 Auftrag im Hintergrund",
                "2 Auftr\u{e4}ge im Hintergrund",
            ],
        ),
        (
            Lang::Fr,
            "ops.background",
            [
                "0 t\u{e2}che en arri\u{e8}re-plan",
                "1 t\u{e2}che en arri\u{e8}re-plan",
                "2 t\u{e2}ches en arri\u{e8}re-plan",
            ],
        ),
        (
            Lang::Es,
            "ops.background",
            [
                "0 trabajos en segundo plano",
                "1 trabajo en segundo plano",
                "2 trabajos en segundo plano",
            ],
        ),
        (
            Lang::En,
            "clipboard.copied",
            [
                "0 entries copied to the clipboard.",
                "1 entry copied to the clipboard.",
                "2 entries copied to the clipboard.",
            ],
        ),
        (
            Lang::De,
            "clipboard.copied",
            [
                "0 Eintr\u{e4}ge in die Zwischenablage kopiert.",
                "1 Eintrag in die Zwischenablage kopiert.",
                "2 Eintr\u{e4}ge in die Zwischenablage kopiert.",
            ],
        ),
        (
            Lang::Fr,
            "clipboard.copied",
            [
                "0 entr\u{e9}e copi\u{e9}e dans le presse-papiers.",
                "1 entr\u{e9}e copi\u{e9}e dans le presse-papiers.",
                "2 entr\u{e9}es copi\u{e9}es dans le presse-papiers.",
            ],
        ),
        (
            Lang::Es,
            "clipboard.copied",
            [
                "0 entradas copiadas al portapapeles.",
                "1 entrada copiada al portapapeles.",
                "2 entradas copiadas al portapapeles.",
            ],
        ),
    ];
    for (lang, key, expected) in cases {
        for (count, want) in expected.iter().enumerate() {
            let text = fill_in(lang, key, &[("count", count.to_string())]);
            assert_eq!(text, *want, "{lang:?} {key} {count}");
        }
    }
}

/// A count read through the grouping of `format::count` still chooses the form, and the other
/// placeholders of a pair are filled as in any text.
#[test]
fn a_grouped_count_and_other_placeholders_fill_a_pair() {
    let text = fill_in(
        Lang::En,
        "queue.title",
        &[
            ("name", "Queue".to_owned()),
            ("count", crate::format::count(1)),
        ],
    );
    assert_eq!(text, "Queue: 1 job");
    let text = fill_in(
        Lang::En,
        "panel.reading",
        &[("count", crate::format::count(1_234))],
    );
    assert_eq!(text, "Reading the folder: 1,234 entries so far");
}

/// A form a translation lacks reads in English, by English's rule: a French 0 missing from the
/// catalogue is the English "0 jobs", not the English "one". A pair neither has falls back to
/// the plain key, and without a `count` the pair is not chosen at all.
#[test]
fn a_missing_plural_form_falls_back_to_english_by_its_rule() {
    let french: Catalogue = [(
        "demo.jobs.other".to_owned(),
        "{count} t\u{e2}ches".to_owned(),
    )]
    .into_iter()
    .collect();
    let english: Catalogue = [
        ("demo.jobs.one".to_owned(), "{count} job".to_owned()),
        ("demo.jobs.other".to_owned(), "{count} jobs".to_owned()),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        plural("demo.jobs", Some(0), Lang::Fr, &french, &english),
        Some("{count} jobs")
    );
    assert_eq!(
        plural("demo.jobs", Some(2), Lang::Fr, &french, &english),
        Some("{count} t\u{e2}ches")
    );
    assert_eq!(
        plural("demo.none", Some(1), Lang::Fr, &french, &english),
        None
    );
    assert_eq!(
        fill("no.such.pair", &[("count", "1".to_owned())]),
        "no.such.pair"
    );
    assert_eq!(fill_in(Lang::Fr, "queue.marked", &[]), "queue.marked");
}
