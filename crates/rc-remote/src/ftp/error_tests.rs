use std::{io, path::Path};

use rc_vfs::VfsError;

use super::{Failure, FtpError, UNSUPPORTED_COMMANDS, reply_text, unsupported_code};

#[test]
fn all_codes_lists_every_code_once() {
    let samples = [
        FtpError::CertificateUnknown(Box::default()),
        FtpError::Connect(io::Error::other("x")),
        FtpError::LoginFailed,
        FtpError::Timeout,
        FtpError::TlsRefused,
        FtpError::Tls(String::new()),
        FtpError::Protocol(String::new()),
        FtpError::TooManySessions(String::new()),
    ];
    for sample in &samples {
        match sample {
            FtpError::CertificateUnknown(_)
            | FtpError::Connect(_)
            | FtpError::LoginFailed
            | FtpError::Timeout
            | FtpError::TlsRefused
            | FtpError::Tls(_)
            | FtpError::Protocol(_)
            | FtpError::TooManySessions(_) => {}
        }
    }
    let codes: Vec<&str> = samples.iter().map(FtpError::code).collect();
    assert_eq!(codes, FtpError::ALL_CODES);
}

#[test]
fn a_session_failure_comes_back_out_of_a_vfs_error() {
    let wrapped = FtpError::LoginFailed.into_vfs(Path::new("/x"));
    assert_eq!(wrapped.code(), "io");
    let inner = FtpError::from_vfs(&wrapped).expect("the FTP error inside");
    assert_eq!(inner.code(), "ftp_login_failed");
}

#[test]
fn refusals_map_to_kinds() {
    let kind = |code: u32, text: &str| {
        Failure::Reply {
            command: "CWD",
            code,
            text: text.to_owned(),
        }
        .into_vfs(Path::new("/f"))
        .code()
    };
    assert_eq!(kind(550, "No such file or directory."), "not_found");
    assert_eq!(kind(550, "Permission denied."), "permission_denied");
    assert_eq!(kind(530, "Not logged in."), "permission_denied");
    assert_eq!(kind(553, "Could not create file."), "invalid_path");
    assert_eq!(kind(502, "Command not implemented."), "unsupported");
    assert_eq!(kind(452, "Insufficient storage space."), "io");
}

#[test]
fn a_command_not_implemented_is_named() {
    let refused = |command: &'static str, code: u32| {
        Failure::Reply {
            command,
            code,
            text: "Command not understood.".to_owned(),
        }
        .into_vfs(Path::new("/f"))
    };
    let error = refused("RETR", 500);
    assert!(
        matches!(
            error,
            VfsError::Unsupported {
                operation: "RETR",
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(unsupported_code(&error), Some("ftp_unsupported_retr"));
    assert_eq!(
        unsupported_code(&refused("PASV", 502)),
        Some("ftp_unsupported_passive")
    );
    // Worked around, never shown: no code of its own.
    assert_eq!(unsupported_code(&refused("MLSD", 500)), None);
    assert_eq!(unsupported_code(&refused("RETR", 550)), None);
    let codes: Vec<&str> = UNSUPPORTED_COMMANDS.iter().map(|(_, code)| *code).collect();
    let mut unique = codes.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), codes.len(), "every code once");
}

#[test]
fn reply_texts_lose_their_code() {
    assert_eq!(reply_text(b"550 No such file.\r\n"), "No such file.");
    assert_eq!(reply_text(b"550"), "550");
}
