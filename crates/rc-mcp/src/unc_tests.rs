//! Network shares from an agent (RC-080-10): the rules on the arguments alone, and the tools over
//! an in-process pipe.

use rc_engine::{Drive, DriveGroup, DriveKind, DriveState, NetworkPath, VfsPath, network_path};
use rmcp::{RoleClient, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::{mapped, network_arguments, refused_whatever_is_mapped};
use crate::test_support::Fixture;

fn object(value: Value) -> rmcp::model::JsonObject {
    value.as_object().cloned().unwrap_or_default()
}

fn named(tool: &str, arguments: Value) -> Vec<String> {
    network_arguments(tool, &object(arguments))
        .into_iter()
        .map(|(text, _)| text)
        .collect()
}

#[test]
fn every_path_argument_is_judged_and_nothing_else() {
    assert_eq!(
        named("list_dir", json!({ "path": r"\\attacker\share" })),
        [r"\\attacker\share"]
    );
    assert_eq!(
        named(
            "ops_copy",
            json!({ "sources": ["C:\\a", "//attacker/share/b"], "target": r"\\?\UNC\x\y" })
        )
        .len(),
        2
    );
    assert_eq!(
        named("terminal_open", json!({ "cwd": r"\\.\pipe\x" })),
        [r"\\.\pipe\x"]
    );
    // Not a path argument, a WSL share, a drive, a remote URL: nothing to refuse.
    for arguments in [
        json!({ "name": r"\\attacker\share" }),
        json!({ "path": r"\\wsl.localhost\Ubuntu\home" }),
        json!({ "path": r"\\wsl$\Debian" }),
        json!({ "path": r"Z:\mapped" }),
        json!({ "path": "sftp://box/home" }),
        json!({ "paths": [], "panel": "left" }),
    ] {
        assert!(
            named("list_dir", arguments.clone()).is_empty(),
            "{arguments}"
        );
    }
}

#[test]
fn a_start_folder_setting_is_a_path_and_another_setting_is_not() {
    let start = json!({ "key": "folders.left_start", "value": r"\\attacker\share" });
    assert_eq!(named("settings_set", start), [r"\\attacker\share"]);
    let theme = json!({ "key": "appearance.theme", "value": r"\\attacker\share" });
    assert!(named("settings_set", theme).is_empty());
    let other_tool = json!({ "key": "folders.left_start", "value": r"\\attacker\share" });
    assert!(named("settings_get", other_tool).is_empty());
}

fn network_drive(letter: &str, remote: Option<&str>) -> Drive {
    Drive {
        id: format!("{letter}:"),
        group: DriveGroup::Drives,
        kind: DriveKind::Network,
        label: format!("{letter}:"),
        root: Some(VfsPath::local(format!("{letter}:\\"))),
        state: DriveState::Unknown,
        volume: None,
        space: None,
        folder: Vec::new(),
        favourite: false,
        remote: remote.map(str::to_owned),
    }
}

#[test]
fn a_share_passes_only_when_a_network_drive_is_mapped_to_it() {
    let drives = [
        network_drive("Y", None),
        network_drive("Z", Some(r"\\FileServer\Team\")),
    ];
    assert!(mapped(r"\\fileserver\team", &drives));
    assert!(!mapped(r"\\fileserver\other", &drives));
    assert!(!mapped(r"\\attacker\share", &drives));
    let mut fixed = network_drive("X", Some(r"\\attacker\share"));
    fixed.kind = DriveKind::Fixed;
    assert!(!mapped(r"\\attacker\share", &[fixed]));
    assert_eq!(
        network_path(r"\\FileServer\Team\docs"),
        Some(NetworkPath::Share(r"\\FileServer\Team".to_owned()))
    );
}

/// Only a share needs the drives: a device or NT path is refused without listing them, so the
/// gate does not dispatch `ListDrives` for it (RC-080-54).
#[test]
fn only_a_share_waits_for_the_drives() {
    let pipe = network_arguments("terminal_open", &object(json!({ "cwd": r"\\.\pipe\x" })));
    assert_eq!(refused_whatever_is_mapped(&pipe), Some(r"\\.\pipe\x"));
    let share = network_arguments("list_dir", &object(json!({ "path": r"\\attacker\share" })));
    assert_eq!(refused_whatever_is_mapped(&share), None);
    let both = network_arguments(
        "ops_copy",
        &object(json!({ "sources": [r"\\fileserver\team\a", r"\\?\GLOBALROOT\x"] })),
    );
    assert_eq!(refused_whatever_is_mapped(&both), Some(r"\\?\GLOBALROOT\x"));
}

struct Setup {
    _dir: TempDir,
    client: RunningService<RoleClient, ()>,
}

async fn setup() -> Setup {
    let built = Fixture::new().build().await;
    let client = built.client().await;
    Setup {
        _dir: built.root,
        client,
    }
}

/// The error code of a call, or `None` when it succeeded.
async fn code(setup: &Setup, tool: &str, arguments: Value) -> Option<String> {
    let result = setup
        .client
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(object(arguments)))
        .await
        .expect("call");
    if result.is_error != Some(true) {
        return None;
    }
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    let body: Value = serde_json::from_str(&text).expect("json");
    body["code"].as_str().map(str::to_owned)
}

#[tokio::test]
async fn list_dir_navigate_search_and_the_start_folder_refuse_a_share() {
    let setup = setup().await;
    for (tool, arguments) in [
        ("list_dir", json!({ "path": r"\\attacker\share" })),
        ("navigate", json!({ "path": r"\\attacker\share\x" })),
        ("navigate", json!({ "path": "//attacker/share" })),
        ("files_search", json!({ "paths": [r"\\attacker\share"] })),
        (
            "settings_set",
            json!({ "key": "folders.left_start", "value": r"\\attacker\share" }),
        ),
    ] {
        assert_eq!(
            code(&setup, tool, arguments.clone()).await.as_deref(),
            Some("network_path"),
            "{tool} {arguments}"
        );
    }
}

#[tokio::test]
async fn a_wsl_share_and_a_local_folder_are_not_refused() {
    let setup = setup().await;
    // Not found off Windows, but never refused as a share.
    for path in [r"\\wsl.localhost\Ubuntu\home", r"\\wsl$\Debian"] {
        for tool in ["list_dir", "navigate"] {
            let answer = code(&setup, tool, json!({ "path": path })).await;
            assert_ne!(answer.as_deref(), Some("network_path"), "{tool} {path}");
        }
    }
    assert_eq!(code(&setup, "list_dir", json!({ "path": "." })).await, None);
    assert_eq!(
        code(
            &setup,
            "settings_set",
            json!({ "key": "folders.left_start", "value": "" })
        )
        .await,
        None
    );
}
