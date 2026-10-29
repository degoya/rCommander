//! Each path field asks for the dialog it needs (open, save, folder) with its file types; the
//! path chosen becomes the field's own input; a cancelled dialog changes nothing.

use std::path::PathBuf;

use rc_engine::{
    SETTINGS,
    settings::{EDITOR_KEY, LEFT_START_KEY, RIGHT_START_KEY, SettingKind, VIEWER_KEY},
};

use super::{BrowseMsg, PathField, chosen};
use crate::{
    app::{
        Message, checksum::ChecksumMsg, connections::ConnMsg, database_dialog::DatabaseMsg,
        harness::Harness, multi_rename::MrMsg, quick_connect::QuickMsg, settings::SettingsMessage,
    },
    platform::file_dialog::{Fallback, Kind},
};

fn setting(key: &str) -> PathField {
    let spec = SETTINGS
        .iter()
        .find(|spec| spec.key == key)
        .expect("the setting exists");
    PathField::Setting(spec)
}

/// Every field with a button, its dialog and the patterns of its file types in order.
fn fields() -> Vec<(PathField, Kind, Vec<&'static str>)> {
    let keys = vec!["id_*;*.pem;*.key;*.ppk", "*.*"];
    let programs = vec!["*.exe;*.cmd;*.bat;*.com", "*.*"];
    vec![
        (PathField::ConnectionKey, Kind::Open, keys.clone()),
        (PathField::QuickKey, Kind::Open, keys),
        (PathField::Backup, Kind::Save, vec!["*.db", "*.*"]),
        (PathField::Restore, Kind::Open, vec!["*.db", "*.*"]),
        (
            PathField::ConfigExport,
            Kind::Save,
            vec!["*.rcexport", "*.*"],
        ),
        (
            PathField::ConfigImport,
            Kind::Open,
            vec!["*.rcexport", "*.*"],
        ),
        (setting(LEFT_START_KEY), Kind::Folder, Vec::new()),
        (setting(RIGHT_START_KEY), Kind::Folder, Vec::new()),
        (setting(VIEWER_KEY), Kind::Open, programs.clone()),
        (setting(EDITOR_KEY), Kind::Open, programs),
        (PathField::SshConfig, Kind::Open, vec!["*.*"]),
        (PathField::Wincmd, Kind::Open, vec!["wincmd.ini", "*.*"]),
        (PathField::WcxFtp, Kind::Open, vec!["wcx_ftp.ini", "*.*"]),
        (PathField::RdmExport, Kind::Open, vec!["*.rdm", "*.*"]),
        (PathField::RenameLog, Kind::Save, vec!["*.log;*.txt", "*.*"]),
        (PathField::ChecksumTarget, Kind::Folder, Vec::new()),
        (
            PathField::PluginPackage,
            Kind::Open,
            vec!["*.rcplug", "*.*"],
        ),
    ]
}

#[test]
fn each_field_asks_for_its_kind_of_dialog_and_its_file_types() {
    for (field, kind, specs) in fields() {
        let request = field.request("typed".to_owned());
        assert_eq!(request.kind, kind, "{field:?}");
        let got: Vec<&str> = request.filters.iter().map(|filter| filter.spec).collect();
        assert_eq!(got, specs, "{field:?}");
        assert!(
            request
                .filters
                .iter()
                .all(|filter| !filter.label.is_empty()),
            "{field:?}: every file type has a text"
        );
        assert_eq!(request.current, "typed", "{field:?}: starts at the field");
    }
}

#[test]
fn key_files_and_ssh_config_start_in_the_ssh_folder() {
    for (field, _, _) in fields() {
        let ssh = matches!(
            field,
            PathField::ConnectionKey | PathField::QuickKey | PathField::SshConfig
        );
        let expected = if ssh {
            Fallback::SshFolder
        } else {
            Fallback::Dialog
        };
        assert_eq!(field.request(String::new()).fallback, expected, "{field:?}");
    }
}

#[test]
fn every_path_setting_has_a_dialog() {
    let paths: Vec<&str> = SETTINGS
        .iter()
        .filter(|spec| matches!(spec.kind, SettingKind::Path { .. }))
        .map(|spec| spec.key)
        .collect();
    let covered: Vec<&str> = fields()
        .into_iter()
        .filter_map(|(field, _, _)| match field {
            PathField::Setting(spec) => Some(spec.key),
            _ => None,
        })
        .collect();
    assert_eq!(paths, covered, "a new path setting needs its row here");
}

#[test]
fn a_cancelled_dialog_leaves_no_message() {
    for (field, _, _) in fields() {
        assert!(chosen(field, None).is_none(), "{field:?}");
    }
}

#[test]
fn the_path_chosen_is_the_fields_own_input() {
    let path = || Some(PathBuf::from("chosen"));
    let typed = |field| chosen(field, path()).expect("a message");
    assert!(matches!(
        typed(PathField::ConnectionKey),
        Message::Connections(ConnMsg::Edit(_))
    ));
    assert!(matches!(
        typed(PathField::QuickKey),
        Message::QuickConnect(QuickMsg::KeyPath(path)) if path == "chosen"
    ));
    for field in [PathField::Backup, PathField::Restore] {
        assert!(matches!(
            typed(field),
            Message::Database(DatabaseMsg::Path(path)) if path == "chosen"
        ));
    }
    assert!(matches!(
        typed(setting(VIEWER_KEY)),
        Message::Settings(SettingsMessage::Edit(key, path)) if key == VIEWER_KEY && path == "chosen"
    ));
    for field in [PathField::SshConfig, PathField::Wincmd, PathField::WcxFtp] {
        assert!(matches!(
            typed(field),
            Message::Connections(ConnMsg::Import(_) | ConnMsg::TcImport(_))
        ));
    }
    assert!(matches!(
        typed(PathField::RenameLog),
        Message::MultiRename(MrMsg::Text(_, path)) if path == "chosen"
    ));
    assert!(matches!(
        typed(PathField::ChecksumTarget),
        Message::Checksum(ChecksumMsg::Target(path)) if path == "chosen"
    ));
    assert!(matches!(
        typed(PathField::PluginPackage),
        Message::Settings(SettingsMessage::Plugins(crate::app::settings::PluginsMsg::Path(path)))
            if path == "chosen"
    ));
}

#[tokio::test]
async fn cancel_keeps_the_field_and_a_choice_fills_it() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::headless(root.path()).await;
    harness.app.open_quick_connect();
    harness
        .app
        .apply(Message::QuickConnect(QuickMsg::KeyPath("typed".to_owned())));
    let key_path = |harness: &Harness| {
        harness
            .app
            .quick
            .as_ref()
            .map(|dialog| dialog.key_path.clone())
            .expect("the dialog is open")
    };
    let effects = harness.app.apply(Message::Browse(BrowseMsg::Picked(
        PathField::QuickKey,
        None,
    )));
    assert!(effects.is_empty());
    assert_eq!(key_path(&harness), "typed", "cancel changes nothing");
    let picked = root.path().join("id_ed25519");
    harness.app.apply(Message::Browse(BrowseMsg::Picked(
        PathField::QuickKey,
        Some(picked.clone()),
    )));
    assert_eq!(key_path(&harness), picked.to_string_lossy());
}
