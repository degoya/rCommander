//! Network shares from an agent (RC-080-10): the rules on the arguments alone, and the tools over
//! an in-process pipe.

use rc_engine::{
    Command, CreateWorkspaceRequest, Drive, DriveGroup, DriveKind, DriveState, NetworkPath,
    VfsPath, WorkspaceSpec, network_path,
};
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
        folder_buttons: Vec::new(),
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

/// A folder of this machine, absolute on every platform.
fn here() -> String {
    std::env::temp_dir().display().to_string()
}

/// A workspace whose left panel has the tab `tab` and, with `cwd`, a terminal starting there.
fn workspace(tab: &str, cwd: Option<&str>) -> Value {
    let location = |path: &str| json!({ "location": { "backend": "local", "path": path } });
    let mut spec = json!({
        "version": 1,
        "active": "left",
        "left": { "tabs": [location(&here()), location(tab)], "active_tab": 0 },
        "right": { "tabs": [location(&here())], "active_tab": 0 }
    });
    if let Some(cwd) = cwd {
        spec["right"]["terminal"] = json!({ "kind": "local", "cwd": cwd });
    }
    spec
}

/// RC-110-21: the folders of a workspace lie below the top level of the call, and are judged.
#[test]
fn a_workspace_spec_is_judged_by_its_tabs_and_start_folders() {
    for tool in ["workspaces_create", "workspaces_update"] {
        assert_eq!(
            named(
                tool,
                json!({ "id": 1, "spec": workspace(r"\\attacker\share", None) })
            ),
            [r"\\attacker\share"],
            "{tool}"
        );
        assert_eq!(
            named(
                tool,
                json!({ "id": 1, "spec": workspace(&here(), Some("//attacker/share/x")) })
            ),
            ["//attacker/share/x"],
            "{tool}"
        );
        // A WSL share, a drive and a folder of this machine stay.
        let wsl = workspace(r"\\wsl.localhost\Ubuntu\home", Some(r"Z:\mapped"));
        assert!(named(tool, json!({ "id": 1, "spec": wsl })).is_empty());
    }
    // A tab on a server is the server's path; another tool's spec is not a workspace.
    let mut remote = workspace(&here(), None);
    remote["left"]["tabs"][1]["location"] =
        json!({ "backend": { "sftp": { "connection_id": 4 } }, "path": "//var/www" });
    assert!(named("workspaces_create", json!({ "spec": remote })).is_empty());
    let share = workspace("//attacker/share", None);
    assert!(named("column_sets_create", json!({ "spec": share })).is_empty());
}

#[tokio::test]
async fn a_workspace_with_a_share_is_refused_from_an_agent_and_kept_from_the_window() {
    let built = Fixture::new().build().await;
    let setup = Setup {
        client: built.client().await,
        _dir: built.root,
    };
    let engine = built.engine;
    // From the window: the person's own share, saved as before.
    let spec = WorkspaceSpec::from_value(workspace("//fileserver/team/docs", None)).expect("reads");
    engine
        .dispatch(Command::CreateWorkspace(CreateWorkspaceRequest {
            name: "Team".to_owned(),
            spec,
            theme_light: None,
            theme_dark: None,
        }))
        .await
        .expect("the window saves a workspace on a share");
    let listed = engine
        .dispatch(Command::ListWorkspaces {})
        .await
        .expect("lists");
    let listed = serde_json::to_value(&listed).expect("json");
    let id = listed["workspaces"][0]["id"].as_i64().expect("an id");
    // From an agent: a tab and a start folder, in a new workspace and in a stored one.
    for spec in [
        workspace("//attacker/share/x", None),
        workspace(r"\\attacker\share", None),
        workspace(&here(), Some("//attacker/share")),
        workspace(&here(), Some(r"\\.\pipe\x")),
    ] {
        for (tool, arguments) in [
            ("workspaces_create", json!({ "name": "Bait", "spec": spec })),
            ("workspaces_update", json!({ "id": id, "spec": spec })),
        ] {
            assert_eq!(
                code(&setup, tool, arguments.clone()).await.as_deref(),
                Some("network_path"),
                "{tool} {arguments}"
            );
        }
    }
    // Nothing was saved, and a workspace without a share still is.
    let listed = engine
        .dispatch(Command::ListWorkspaces {})
        .await
        .expect("lists");
    let listed = serde_json::to_value(&listed).expect("json");
    assert_eq!(listed["workspaces"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        listed["workspaces"][0]["spec"]["left"]["tabs"][1]["location"]["path"],
        "//fileserver/team/docs"
    );
    let plain = workspace(&here(), Some(&here()));
    assert_eq!(
        code(
            &setup,
            "workspaces_update",
            json!({ "id": id, "spec": plain })
        )
        .await,
        None
    );
}
