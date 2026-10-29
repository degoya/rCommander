//! Where the manager starts and what typing in its tree does (RC-050-15), and what the
//! duplicate's answer does to the tree.

use rc_engine::{
    Chord, Connection, ConnectionId, ConnectionTree, Outcome, SettingValue, TreeItem,
    settings::CONNECTIONS_FOCUS_LIST_KEY,
};
use serde_json::{Value, json};

use crate::app::connections::{ConnectionManager, Focus, NameFor, Pending, tree::TreeState};

/// A connection as the engine answers it, through serde as `tree_tests.rs` builds them.
fn connection(id: i64, folder: Option<i64>, name: &str) -> Value {
    json!({
        "id": id, "folder_id": folder, "name": name, "kind": "ssh", "host": "example.org",
        "port": 22, "username": null, "auth_method": "password", "key_path": null,
        "login_script_id": null, "default_view": "files", "start_directory": null,
        "notes": null, "sort_order": id, "created_at": "", "updated_at": "",
        "has_password": false, "has_key": false, "has_passphrase": false, "key": null
    })
}

/// `work/` (with `db/` holding `pg`, and `web`) and `nas` at the root; `work/` and `db/` closed.
fn fixture(last_used: Option<i64>) -> ConnectionManager {
    let tree: ConnectionTree = serde_json::from_value(json!({
        "folders": [{
            "id": 1, "parent_id": null, "name": "work", "sort_order": 1, "created_at": "",
            "updated_at": "",
            "folders": [{
                "id": 3, "parent_id": 1, "name": "db", "sort_order": 1, "created_at": "",
                "updated_at": "", "folders": [], "connections": [connection(2, Some(3), "pg")]
            }],
            "connections": [connection(1, Some(1), "web")]
        }],
        "connections": [connection(3, None, "nas")],
        "last_used": last_used
    }))
    .expect("a tree");
    let mut state = TreeState::default();
    state.tree = tree;
    state.fold(rc_engine::FolderId(1), Some(false));
    state.fold(rc_engine::FolderId(3), Some(false));
    ConnectionManager {
        tree: state,
        focus: Focus::Tree,
        ..ConnectionManager::default()
    }
}

fn chord(text: &str) -> Chord {
    text.parse().expect("chord")
}

#[test]
fn the_cursor_starts_on_the_connection_used_last_with_its_folders_open() {
    let mut manager = fixture(Some(2));
    manager.place_start();
    assert_eq!(
        manager.tree.cursor,
        Some(TreeItem::Connection(ConnectionId(2)))
    );
    assert!(manager.tree.is_open(rc_engine::FolderId(1)));
    assert!(manager.tree.is_open(rc_engine::FolderId(3)));

    // A later tree leaves the cursor where the person moved it.
    manager.tree.cursor = Some(TreeItem::Connection(ConnectionId(3)));
    manager.place_start();
    assert_eq!(
        manager.tree.cursor,
        Some(TreeItem::Connection(ConnectionId(3)))
    );
}

#[test]
fn the_row_of_this_run_comes_first_and_a_missing_one_falls_back_to_the_first_row() {
    let mut manager = fixture(Some(2));
    manager.start = Some(TreeItem::Connection(ConnectionId(3)));
    manager.place_start();
    assert_eq!(
        manager.tree.cursor,
        Some(TreeItem::Connection(ConnectionId(3)))
    );

    let mut manager = fixture(Some(99));
    manager.start = Some(TreeItem::Connection(ConnectionId(98)));
    manager.place_start();
    assert_eq!(
        manager.tree.cursor,
        manager.tree.rows().first().map(|row| row.item)
    );
}

#[test]
fn the_setting_off_puts_the_keyboard_in_the_filter() {
    let answer = |on: bool| -> Result<Outcome, &'static str> {
        let setting: SettingValue = serde_json::from_value(json!({
            "setting": CONNECTIONS_FOCUS_LIST_KEY, "value": on, "default": true,
            "type": "bool", "protected": false, "restart": false
        }))
        .expect("a setting");
        Ok(Outcome::Settings {
            settings: vec![setting],
        })
    };
    let mut manager = fixture(None);
    assert!(manager.focus_setting(answer(true)).is_empty());
    assert_eq!(manager.focus, Focus::Tree);
    assert!(manager.focus_setting(Err("db_missing")).is_empty());
    assert_eq!(
        manager.focus,
        Focus::Tree,
        "a setting that does not read keeps the list"
    );
    assert_eq!(manager.focus_setting(answer(false)).len(), 1);
    assert_eq!(manager.focus, Focus::Filter);
}

#[test]
fn a_character_typed_in_the_tree_goes_into_the_filter() {
    let mut manager = fixture(None);
    assert!(manager.type_into_filter(&chord("w")).is_some());
    assert_eq!(manager.focus, Focus::Filter);
    assert!(manager.type_into_filter(&chord("Shift+e")).is_some());
    assert!(manager.type_into_filter(&chord("2")).is_some());
    assert_eq!(manager.tree.filter, "wE2");
    for other in ["Space", "Ctrl+w", "Alt+w", "F5", "Enter", "Shift+1"] {
        assert!(manager.type_into_filter(&chord(other)).is_none(), "{other}");
    }
    assert_eq!(manager.tree.filter, "wE2");
}

#[test]
fn shift_f5_duplicates_a_connection_and_says_so_elsewhere() {
    let mut manager = fixture(None);
    manager.tree.cursor = Some(TreeItem::Connection(ConnectionId(3)));
    assert_eq!(manager.key(&chord("Shift+F5"), false).len(), 1);
    assert_eq!(manager.note, None);

    manager.tree.cursor = Some(TreeItem::Folder(rc_engine::FolderId(1)));
    assert!(manager.key(&chord("Shift+F5"), false).is_empty());
    assert!(manager.note.as_ref().is_some_and(|(_, error)| *error));
}

#[test]
fn the_copy_takes_the_cursor_with_its_name_open_and_a_locked_vault_asks() {
    let mut manager = fixture(None);
    let copy: Connection = serde_json::from_value(connection(7, None, "nas (2)")).expect("copy");
    manager.duplicated(
        ConnectionId(3),
        Ok(Outcome::Connection { connection: copy }),
    );
    assert_eq!(
        manager.tree.cursor,
        Some(TreeItem::Connection(ConnectionId(7)))
    );
    assert_eq!(
        manager.pending,
        Some(Pending::Name {
            target: NameFor::Rename(TreeItem::Connection(ConnectionId(7))),
            text: "nas (2)".into(),
        })
    );

    let mut manager = fixture(None);
    manager.duplicated(ConnectionId(3), Err("vault_locked"));
    assert!(manager.prompt.is_some());
    assert_eq!(manager.pending, None);
}
