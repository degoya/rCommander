//! A place, a start folder or a key path of an import on a share - in any spelling - is never
//! taken without the person's tick (RC-080-19, the rule of RC-080-10).

use rc_vfs::VfsPath;
use serde_json::json;

use super::on_share;
use crate::{
    commands::exchange_fixture::{engine_in, import, report},
    exchange::RowStatus,
};

/// Every spelling of a share or a device path, `/` and `\` mixed.
const SHARES: &[&str] = &[
    r"\\server\share\x",
    "//server/share/x",
    r"\/server/share",
    r"/\server\share",
    r"\\?\UNC\server\share\x",
    r"\\.\UNC\server\share",
    r"\\.\pipe\x",
    r"\??\UNC\server\share",
    r"\\?\GLOBALROOT\Device\x",
];

#[test]
fn every_spelling_of_a_share_is_one() {
    for share in SHARES {
        assert!(on_share(&VfsPath::local(*share)), "{share}");
    }
    for local in [
        r"C:\Users\me",
        "/home/me",
        r"\\?\C:\Users\me",
        r"\\wsl.localhost\Ubuntu\home",
        "relative",
    ] {
        assert!(!on_share(&VfsPath::local(local)), "{local}");
    }
}

#[tokio::test]
async fn shares_wait_for_a_tick_in_hotlist_tabs_settings_and_key_paths() {
    let dir = tempfile::tempdir().expect("dir");
    let engine = engine_in(dir.path()).await;
    let local = dir.path().join("here");
    let mut items = vec![json!({"name": "local", "location": VfsPath::local(&local)})];
    for (at, share) in SHARES.iter().enumerate() {
        items.push(json!({"name": format!("share{at}"), "location": VfsPath::local(*share)}));
    }
    let content = json!({
        "origin": "abc",
        "secrets": false,
        // Absolute on every platform, so the row's check passes and the share is judged.
        "settings": [{"setting": "folders.left_start", "value": "//server/share"}],
        "connections": {
            "connections": [{
                "source": "rcexport:abc", "key": "1",
                "connection": {"name": "web", "host": "web.example.org", "auth_method": "key",
                               "key_path": r"\\?\UNC\server\share\id_ed25519"}
            }],
            "login_scripts": []
        },
        "hotlist": {
            "items": items,
            "tabs": [{"panel": "left", "location": VfsPath::local(r"/\server\share")}]
        }
    });
    let file = json!({
        "format": "rcommander-export", "version": 1, "created_at": "", "content": content
    });
    let path = dir.path().join("shares.rcexport");
    std::fs::write(&path, file.to_string()).expect("written");
    let done = report(&engine, import(&path, None, false, None)).await;
    for row in &done.rows {
        if row.name == "local" {
            assert_eq!(row.status, RowStatus::Created, "{row:?}");
            continue;
        }
        assert_eq!(row.warning.as_deref(), Some("network_location"), "{row:?}");
        assert!(!row.selected, "{row:?}");
        assert!(
            matches!(row.status, RowStatus::New | RowStatus::Changed),
            "{row:?}"
        );
    }
    assert_eq!(done.rows.len(), SHARES.len() + 4);
    let hotlist = engine.db().expect("db").hotlist().await.expect("read");
    assert_eq!(hotlist.len(), 1, "only the local entry");
}
