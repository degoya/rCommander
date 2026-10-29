use super::{NetworkDriveError, from_system, parse_letter, parse_remote, suggest_letter};
use crate::{Drive, DriveGroup, DriveKind, DriveState, EngineError};

fn drive(id: &str, group: DriveGroup) -> Drive {
    Drive {
        id: id.to_owned(),
        group,
        kind: DriveKind::Fixed,
        label: id.to_owned(),
        root: None,
        state: DriveState::Unknown,
        volume: None,
        space: None,
        folder: Vec::new(),
        favourite: false,
        remote: None,
    }
}

fn reason(text: &str) -> &'static str {
    match parse_remote(text) {
        Err(NetworkDriveError::PathInvalid { reason }) => reason,
        other => panic!("{text:?} refused, got {other:?}"),
    }
}

#[test]
fn a_share_and_a_folder_below_it_are_network_folders() {
    assert_eq!(parse_remote(r"\\nas\media").expect("share"), r"\\nas\media");
    assert_eq!(
        parse_remote(r"  \\nas.local\media\films\  ").expect("folder"),
        r"\\nas.local\media\films"
    );
    assert_eq!(
        parse_remote("//192.168.1.5/backup").expect("slashes"),
        r"\\192.168.1.5\backup"
    );
    assert_eq!(
        parse_remote(r"\\files@SSL@443\DavWWWRoot").expect("WebDAV"),
        r"\\files@SSL@443\DavWWWRoot"
    );
}

#[test]
fn what_is_not_unc_is_refused_with_a_reason() {
    assert!(reason("").contains("two backslashes"));
    assert!(reason(r"C:\data").contains("two backslashes"));
    assert!(reason(r"\nas\media").contains("two backslashes"));
    assert!(reason(r"\\nas").contains("server and a share"));
    assert!(reason(r"\\nas\").contains("server and a share"));
    assert!(reason(r"\\?\C:\data").contains("device path"));
    assert!(reason(r"\\.\pipe\x").contains("device path"));
    assert!(reason(r"\\nas\\media").contains("empty"));
    assert!(reason(r"\\nas\media\..\other").contains(". or .."));
}

#[test]
fn characters_no_name_has_are_refused() {
    for text in [
        r"\\nas\me:dia",
        r"\\nas\me*dia",
        r"\\nas\me?dia",
        r#"\\nas\me"dia"#,
        "\\\\nas\\me\u{0}dia",
        "\\\\nas\\media\nnet use",
        r"\\nas\a|b",
    ] {
        assert!(reason(text).contains("character"), "{text:?}");
    }
    let long = format!(r"\\nas\{}", "a".repeat(1100));
    assert!(reason(&long).contains("too long"));
}

#[test]
fn a_letter_is_one_of_a_to_z_and_free() {
    let drives = [
        drive("C:", DriveGroup::Drives),
        drive("Z:", DriveGroup::Drives),
        // Not a letter of the Drives group: a WSL distribution's id.
        drive(r"\\wsl.localhost\y", DriveGroup::Wsl),
    ];
    assert_eq!(parse_letter("y", &drives).expect("free"), 'Y');
    assert_eq!(parse_letter(" x: ", &drives).expect("free"), 'X');
    assert_eq!(parse_letter(r"m:\", &drives).expect("free"), 'M');
    for taken in ["z", "Z:", "c:\\"] {
        assert!(
            matches!(
                parse_letter(taken, &drives),
                Err(NetworkDriveError::LetterInUse { .. })
            ),
            "{taken}"
        );
    }
    for invalid in ["", "1", "zz", "\u{e4}", "Z:x", r"\\nas\media"] {
        let error = parse_letter(invalid, &drives).expect_err(invalid);
        assert_eq!(error.code(), "drive_letter_invalid", "{invalid}");
    }
}

#[test]
fn the_suggestion_is_the_last_free_letter() {
    assert_eq!(
        suggest_letter(&[drive("C:", DriveGroup::Drives)]),
        Some('Z')
    );
    let busy = [
        drive("Z:", DriveGroup::Drives),
        drive("Y:", DriveGroup::Drives),
    ];
    assert_eq!(suggest_letter(&busy), Some('X'));
    let all: Vec<Drive> = ('C'..='Z')
        .map(|letter| drive(&format!("{letter}:"), DriveGroup::Drives))
        .collect();
    assert_eq!(suggest_letter(&all), None);
}

#[test]
fn windows_answers_become_stable_codes() {
    let code = |answer: u32| from_system(answer, 'Z', r"\\nas\media").code();
    assert!(matches!(
        from_system(1223, 'Z', r"\\nas\media"),
        EngineError::Cancelled
    ));
    assert_eq!(code(85), "drive_letter_in_use");
    assert_eq!(code(67), "network_path_not_found");
    assert_eq!(code(53), "network_path_not_found");
    assert_eq!(code(1219), "network_credentials_conflict");
    assert_eq!(code(1326), "network_access_denied");
    assert_eq!(code(2401), "network_drive_in_use");
    assert_eq!(code(2404), "network_drive_in_use");
    assert_eq!(code(2250), "drive_not_network");
    assert_eq!(code(50), "network_drives_unsupported");
    assert_eq!(code(31), "network_drive_failed");
    let failed = from_system(31, 'Z', r"\\nas\media");
    assert!(failed.to_string().contains("31"), "{failed}");
}

#[test]
fn every_code_is_listed_once() {
    let samples = [
        NetworkDriveError::PathInvalid { reason: "x" },
        NetworkDriveError::LetterInvalid {
            letter: "1".to_owned(),
        },
        NetworkDriveError::LetterInUse { letter: 'Z' },
        NetworkDriveError::NotNetwork {
            drive: "C:".to_owned(),
        },
        NetworkDriveError::InUse {
            drive: "Z:".to_owned(),
        },
        NetworkDriveError::PathNotFound {
            remote: r"\\nas\x".to_owned(),
        },
        NetworkDriveError::CredentialConflict {
            remote: r"\\nas\x".to_owned(),
        },
        NetworkDriveError::AccessDenied {
            remote: r"\\nas\x".to_owned(),
        },
        NetworkDriveError::Unsupported,
        NetworkDriveError::Failed { code: 31 },
    ];
    let codes: Vec<&str> = samples.iter().map(NetworkDriveError::code).collect();
    assert_eq!(codes, NetworkDriveError::ALL_CODES);
}
