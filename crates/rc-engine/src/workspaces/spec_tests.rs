use rc_vfs::{ArchiveFormat, BackendId, VfsPath};
use serde_json::json;

use super::*;
use crate::{ConnectionId, ListMode, PanelId, PanelView, SortOrder};

fn context(ids: &[i64]) -> SpecContext {
    SpecContext {
        connections: ids.iter().copied().map(ConnectionId).collect(),
    }
}

fn code(result: Result<(), WorkspaceError>) -> &'static str {
    result.expect_err("refused").code()
}

fn on_server(connection_id: i64, path: &str) -> TabSpec {
    TabSpec {
        location: VfsPath {
            backend: BackendId::Sftp { connection_id },
            path: path.into(),
        },
        sort: None,
        mode: None,
        locked: false,
    }
}

#[test]
fn a_sample_spec_passes_and_reads_back_from_its_json() {
    let spec = sample();
    spec.check(&context(&[])).expect("fits");
    let text = spec.to_json().expect("json");
    assert_eq!(WorkspaceSpec::parse(&text).expect("reads"), spec);
}

#[test]
fn the_json_has_the_contracts_shape_and_defaults() {
    let text = json!({
        "version": 1,
        "active": "left",
        "left": {
            "tabs": [{ "location": { "backend": "local", "path": "/srv" } }],
            "active_tab": 0
        },
        "right": {
            "tabs": [{
                "location": { "backend": { "sftp": { "connection_id": 4 } }, "path": "/var/www" },
                "sort": { "key": "size", "descending": true },
                "mode": "brief",
                "locked": true
            }],
            "active_tab": 0,
            "view": "terminal",
            "terminal": { "kind": "ssh", "connection_id": 4, "cwd": "/var/www" }
        }
    })
    .to_string();
    let spec = WorkspaceSpec::parse(&text).expect("reads");
    assert_eq!(spec.active, PanelId::Left);
    assert_eq!(spec.split_permille, None, "the divider stays when left out");
    assert_eq!(spec.left.view, PanelView::Files);
    assert_eq!(spec.left.terminal, None);
    assert_eq!(spec.left.tabs[0].sort, None);
    assert!(!spec.left.tabs[0].locked);
    let right = &spec.right.tabs[0];
    assert_eq!(right.mode, Some(ListMode::Brief));
    assert!(right.locked);
    assert_ne!(right.sort, Some(SortOrder::default()));
    assert_eq!(
        spec.right.terminal,
        Some(TermSpec::Ssh {
            connection_id: ConnectionId(4),
            cwd: Some("/var/www".into())
        })
    );
    assert_eq!(
        spec.connections().into_iter().collect::<Vec<_>>(),
        [ConnectionId(4)]
    );
    #[cfg(unix)]
    spec.check(&context(&[4])).expect("fits");
    let local: TermSpec =
        serde_json::from_value(json!({ "kind": "local" })).expect("a local default shell");
    assert_eq!(
        local,
        TermSpec::Local {
            shell: None,
            cwd: None
        }
    );
}

#[test]
fn another_version_is_refused_before_its_fields() {
    for text in [
        json!({ "version": 2, "whatever": true }).to_string(),
        json!({ "active": "left" }).to_string(),
    ] {
        let refused = WorkspaceSpec::parse(&text).expect_err("refused");
        assert_eq!(refused.code(), "workspace_version", "{text}");
    }
    let mut spec = sample();
    spec.version = 2;
    assert_eq!(code(spec.check(&context(&[]))), "workspace_version");
    let broken = json!({ "version": 1, "active": "middle" }).to_string();
    assert_eq!(
        WorkspaceSpec::parse(&broken).expect_err("refused").code(),
        "workspace_invalid"
    );
    assert_eq!(
        WorkspaceSpec::parse("{").expect_err("refused").code(),
        "workspace_invalid"
    );
}

#[test]
fn a_panel_has_one_to_thirty_two_tabs_and_shows_one_of_them() {
    let mut spec = sample();
    spec.left.tabs.clear();
    assert_eq!(code(spec.check(&context(&[]))), "workspace_tabs");
    let mut spec = sample();
    let tab = spec.left.tabs[0].clone();
    spec.left.tabs = vec![tab; MAX_TABS];
    spec.check(&context(&[])).expect("32 fit");
    spec.left.tabs.push(spec.left.tabs[0].clone());
    assert_eq!(code(spec.check(&context(&[]))), "workspace_tabs");
    let mut spec = sample();
    spec.right.active_tab = 1;
    assert_eq!(code(spec.check(&context(&[]))), "workspace_tabs");
}

#[test]
fn the_divider_stays_within_its_bounds() {
    for (split, fits) in [(99, false), (100, true), (900, true), (901, false)] {
        let mut spec = sample();
        spec.split_permille = Some(split);
        let result = spec.check(&context(&[]));
        assert_eq!(result.is_ok(), fits, "{split}");
        if !fits {
            assert_eq!(code(result), "workspace_invalid");
        }
    }
}

#[test]
fn the_terminal_view_needs_a_terminal_that_fits() {
    let mut spec = sample();
    spec.left.view = PanelView::Terminal;
    assert_eq!(code(spec.check(&context(&[]))), "workspace_terminal");
    for (terminal, wanted) in [
        (
            TermSpec::Local {
                shell: Some("bash".into()),
                cwd: None,
            },
            None,
        ),
        (
            TermSpec::Wsl {
                distro: "Ubuntu-24.04".into(),
                cwd: Some("/home".into()),
            },
            None,
        ),
        (
            TermSpec::Local {
                shell: Some(" bash".into()),
                cwd: None,
            },
            Some("workspace_terminal"),
        ),
        (
            TermSpec::Wsl {
                distro: "../etc".into(),
                cwd: None,
            },
            Some("workspace_terminal"),
        ),
        (
            TermSpec::Local {
                shell: None,
                cwd: Some("/tmp\ncd /".into()),
            },
            Some("workspace_terminal"),
        ),
        (
            TermSpec::Ssh {
                connection_id: ConnectionId(9),
                cwd: None,
            },
            Some("workspace_connection"),
        ),
    ] {
        let mut spec = sample();
        spec.left.view = PanelView::Terminal;
        spec.left.terminal = Some(terminal.clone());
        let result = spec.check(&context(&[]));
        match wanted {
            None => result.expect("fits"),
            Some(wanted) => assert_eq!(code(result), wanted, "{terminal:?}"),
        }
    }
}

#[test]
fn a_tab_is_on_this_machine_or_a_known_server_never_in_an_archive() {
    let mut spec = sample();
    spec.right.tabs.push(on_server(3, "/home"));
    assert_eq!(code(spec.check(&context(&[]))), "workspace_connection");
    spec.check(&context(&[3])).expect("a known server");

    let mut spec = sample();
    let outer = spec.left.tabs[0].location.join("backup.zip");
    spec.left.tabs[0].location = VfsPath::archive(outer, ArchiveFormat::Zip, "/");
    assert_eq!(code(spec.check(&context(&[]))), "workspace_location");

    let mut spec = sample();
    spec.left.tabs[0].location = VfsPath::local("relative/folder");
    assert_eq!(code(spec.check(&context(&[]))), "workspace_location");

    let mut spec = sample();
    spec.left.tabs[0].location = VfsPath::local("");
    assert_eq!(code(spec.check(&context(&[]))), "workspace_location");
}

#[test]
fn a_spec_over_64_kib_is_refused() {
    let mut spec = sample();
    let long = "x".repeat(MAX_TEXT - 200);
    let tab = spec.left.tabs[0].clone();
    spec.left.tabs = (0..MAX_TABS)
        .map(|_| TabSpec {
            location: tab.location.join(&long),
            ..tab.clone()
        })
        .collect();
    assert_eq!(code(spec.check(&context(&[]))), "workspace_too_large");
    let text = "x".repeat(MAX_SPEC_BYTES + 1);
    assert_eq!(
        WorkspaceSpec::parse(&text).expect_err("refused").code(),
        "workspace_too_large"
    );
}

#[test]
fn a_name_is_trimmed_short_and_without_control_characters() {
    assert_eq!(clean_name("  Servers ").expect("fits"), "Servers");
    assert_eq!(
        clean_name(&"\u{e4}".repeat(MAX_NAME_CHARS))
            .expect("fits")
            .chars()
            .count(),
        MAX_NAME_CHARS
    );
    for name in [
        "",
        "   ",
        "two\nlines",
        "tab\there",
        &"x".repeat(MAX_NAME_CHARS + 1),
    ] {
        assert_eq!(
            clean_name(name).expect_err("refused").code(),
            "workspace_name",
            "{name:?}"
        );
    }
}

#[test]
fn the_apply_mode_reads_its_setting() {
    assert_eq!(ApplyMode::of_setting("replace_all"), ApplyMode::ReplaceAll);
    assert_eq!(ApplyMode::of_setting("add"), ApplyMode::Add);
    assert_eq!(
        ApplyMode::of_setting("replace_keep_locked"),
        ApplyMode::ReplaceKeepLocked
    );
    assert_eq!(ApplyMode::of_setting("other"), ApplyMode::default());
    assert_eq!(ApplyMode::default(), ApplyMode::ReplaceKeepLocked);
}
