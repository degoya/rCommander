use super::{Catalogue, english, error_text, fill, lookup, resolve};

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
        .chain(rc_remote::SshError::ALL_CODES)
        .chain(rc_remote::SftpError::ALL_CODES)
        .chain(rc_engine::viewer::PatternError::ALL_CODES)
        .chain(rc_engine::LoginScriptError::ALL_CODES)
        .chain(rc_engine::ScriptInvalid::ALL_CODES)
        .chain(rc_remote::login_script::ScriptError::ALL_CODES)
        .chain(rc_mcp::ALL_CODES)
        .chain(rc_engine::plugins::ALL_CODES);
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
