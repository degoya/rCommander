use crate::{Command, CommandClass};

#[test]
fn the_ssh_config_import_reads_with_defaults_and_is_write() {
    let command: Command =
        serde_json::from_str(r#"{"command":"connections.import_ssh_config","params":{}}"#)
            .expect("deserializes");
    assert_eq!(
        command,
        Command::ImportSshConfig(crate::SshConfigImportRequest {
            path: None,
            dry_run: false,
            hosts: None,
            folder_id: None,
        })
    );
    assert_eq!(command.spec().class, CommandClass::Write);
}

#[test]
fn the_total_commander_import_reads_with_defaults_and_its_passwords_are_gui_only() {
    let command: Command =
        serde_json::from_str(r#"{"command":"import.total_commander","params":{}}"#)
            .expect("deserializes");
    let Command::ImportTotalCommander(request) = &command else {
        panic!("the import, got {command:?}");
    };
    assert!(request.parts.hotlist && request.parts.tabs && request.parts.connections);
    assert!(!request.dry_run);
    assert_eq!(
        request.tabs_mode,
        crate::import::total_commander_report::TcTabsMode::Append
    );
    // It writes, so it is registered a write, its preview included (RC-080-12).
    assert_eq!(command.spec().class, CommandClass::Write);
    let passwords: Command =
        serde_json::from_str(r#"{"command":"import.total_commander_passwords","params":{}}"#)
            .expect("deserializes");
    assert_eq!(passwords.spec().class, CommandClass::GuiOnly);
}

#[test]
fn the_remote_desktop_manager_import_needs_a_path_and_is_write() {
    let command: Command = serde_json::from_str(
        r#"{"command":"import.remote_desktop_manager","params":{"path":"a.rdm"}}"#,
    )
    .expect("deserializes");
    assert_eq!(
        command,
        Command::ImportRemoteDesktopManager(crate::RdmImportRequest {
            path: "a.rdm".into(),
            dry_run: false,
            only: None,
        })
    );
    assert_eq!(command.spec().class, CommandClass::Write);
    assert!(
        serde_json::from_str::<Command>(
            r#"{"command":"import.remote_desktop_manager","params":{}}"#
        )
        .is_err()
    );
}
